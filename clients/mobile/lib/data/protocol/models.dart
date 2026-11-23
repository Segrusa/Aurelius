// GENERATED CODE - DO NOT EDIT.
// Run: cargo xtask gen-dart
// ignore_for_file: unnecessary_cast, unnecessary_null_aware_operator, unnecessary_non_null_assertion, constant_identifier_names, use_null_aware_elements, unintended_html_in_doc_comment

/// What an employee's runs may do with files and commands (C3), next to its mode:
/// the level says which tools a run has, the mode which of those are asked first.
enum AccessLevel {
  /// Every work tool, as a run has without a level of its own.
  inherit('inherit'),
  /// Also changes files; no commands or web.
  readEdit('read_edit'),
  /// Reads and searches the folder; no edits, commands or web.
  readOnly('read_only'),
  /// A level of a newer server this client does not know. A run treats it as read-only.
  unknown('unknown');

  const AccessLevel(this.wire);

  final String wire;

  static AccessLevel fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown AccessLevel value: $value'),
      );

  String toJson() => wire;
}

/// One run that really runs now (feature `activity_overview`).
final class ActivityBusy {
  const ActivityBusy({
    this.employeeId,
    required this.employeeName,
    this.employeePaused,
    required this.sinceMs,
    required this.work,
  });

  /// None: the project assistant, which is no employee.
  final String? employeeId;
  final String employeeName;
  /// It was paused during this run: the run finishes, nothing new starts.
  final bool? employeePaused;
  final int sinceMs;
  final ActivityWork work;

  factory ActivityBusy.fromJson(Map<String, dynamic> json) => ActivityBusy(
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    employeeName: (json['employee_name'] as String),
    employeePaused: (json['employee_paused'] == null ? null : (json['employee_paused'] as bool)),
    sinceMs: (json['since_ms'] as num).toInt(),
    work: ActivityWork.fromJson(json['work'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
    'employee_name': employeeName,
    if (employeePaused != null) 'employee_paused': employeePaused,
    'since_ms': sinceMs,
    'work': work.toJson(),
  };
}

/// One delivery, queue step or run step (R8; feature `activity_log`).
final class ActivityLogEntry {
  const ActivityLogEntry({
    required this.atMs,
    required this.category,
    this.detail,
    this.employeeId,
    this.employeeName,
    this.hop,
    this.place,
    this.runId,
    required this.seq,
    required this.step,
    this.threadKey,
  });

  final int atMs;
  /// delivery, queue or run. Text, not an enum: a later value only shows as it is.
  final String category;
  /// A short code or a tool's name; never the text of a message or a tool's input.
  final String? detail;
  final String? employeeId;
  final String? employeeName;
  final int? hop;
  final ThreadPlace? place;
  final String? runId;
  final int seq;
  /// What happened (delivered, claimed, started, tool, ...). Text for the same reason.
  final String step;
  final String? threadKey;

  factory ActivityLogEntry.fromJson(Map<String, dynamic> json) => ActivityLogEntry(
    atMs: (json['at_ms'] as num).toInt(),
    category: (json['category'] as String),
    detail: (json['detail'] == null ? null : (json['detail'] as String)),
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    employeeName: (json['employee_name'] == null ? null : (json['employee_name'] as String)),
    hop: (json['hop'] == null ? null : (json['hop'] as num).toInt()),
    place: (json['place'] == null ? null : ThreadPlace.fromJson(json['place'] as Map<String, dynamic>)),
    runId: (json['run_id'] == null ? null : (json['run_id'] as String)),
    seq: (json['seq'] as num).toInt(),
    step: (json['step'] as String),
    threadKey: (json['thread_key'] == null ? null : (json['thread_key'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'at_ms': atMs,
    'category': category,
    if (detail != null) 'detail': detail,
    if (employeeId != null) 'employee_id': employeeId,
    if (employeeName != null) 'employee_name': employeeName,
    if (hop != null) 'hop': hop,
    if (place != null) 'place': place?.toJson(),
    if (runId != null) 'run_id': runId,
    'seq': seq,
    'step': step,
    if (threadKey != null) 'thread_key': threadKey,
  };
}

/// `activity_log.list`: the newest rows that match, newest first.
final class ActivityLogPage {
  const ActivityLogPage({
    required this.asOfSeq,
    required this.entries,
    this.hasMore = false,
  });

  final int asOfSeq;
  final List<ActivityLogEntry> entries;
  /// There are older rows: ask again with `before_seq` as the last row's `seq`.
  final bool hasMore;

  factory ActivityLogPage.fromJson(Map<String, dynamic> json) => ActivityLogPage(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    entries: (json['entries'] as List<dynamic>).map((x0) => ActivityLogEntry.fromJson(x0 as Map<String, dynamic>)).toList(),
    hasMore: (json['has_more'] == null ? false : (json['has_more'] as bool)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'entries': entries.map((x0) => x0.toJson()).toList(),
    'has_more': hasMore,
  };
}

/// `activity.overview` (feature `activity_overview`): who is busy, what waits and
/// on what, and the state of each ticket with work going or held up. A ticket
/// that is not listed has nothing going on.
final class ActivityOverview {
  const ActivityOverview({
    required this.asOfSeq,
    required this.busy,
    required this.tickets,
    required this.waiting,
  });

  final int asOfSeq;
  final List<ActivityBusy> busy;
  final List<TicketActivity> tickets;
  final List<ActivityWaiting> waiting;

  factory ActivityOverview.fromJson(Map<String, dynamic> json) => ActivityOverview(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    busy: (json['busy'] as List<dynamic>).map((x0) => ActivityBusy.fromJson(x0 as Map<String, dynamic>)).toList(),
    tickets: (json['tickets'] as List<dynamic>).map((x0) => TicketActivity.fromJson(x0 as Map<String, dynamic>)).toList(),
    waiting: (json['waiting'] as List<dynamic>).map((x0) => ActivityWaiting.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'busy': busy.map((x0) => x0.toJson()).toList(),
    'tickets': tickets.map((x0) => x0.toJson()).toList(),
    'waiting': waiting.map((x0) => x0.toJson()).toList(),
  };
}

/// What an employee is doing now (U6): the dot in the list. Derived, never stored; a
/// paused employee is in `status`.
enum ActivityState {
  /// Nothing for it to do.
  idle('idle'),
  /// There is work, but nothing runs yet (its turn, a usage limit, the machine).
  queued('queued'),
  /// A state of a newer server this client does not know.
  unknown('unknown'),
  /// It waits for a board member: an approval, or an answer to a question.
  waiting('waiting'),
  /// A run of it is going.
  working('working');

  const ActivityState(this.wire);

  final String wire;

  static ActivityState fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown ActivityState value: $value'),
      );

  String toJson() => wire;
}

/// One tool use during an answer, shown at its place in the text.
final class ActivityView {
  const ActivityView({
    required this.callId,
    required this.input,
    required this.offset,
    this.ok,
    this.output,
    this.outputBytes,
    this.parentCallId,
    required this.tool,
    this.truncated,
  });

  final String callId;
  final dynamic input;
  /// Byte position in the answer text where it happened.
  final int offset;
  /// `None` while running.
  final bool? ok;
  /// Result text (capped); `None` while running.
  final String? output;
  /// The whole output's length in bytes, when `output` was shortened.
  final int? outputBytes;
  /// The `Task`/`Agent` call of the subagent that made this call (its
  /// `AgentView::call_id`); none for the employee's own calls (feature
  /// `subagent_steps`). Older servers leave it out.
  final String? parentCallId;
  /// The runtime's tool name, e.g. "Bash", "Edit", "WebSearch".
  final String tool;
  /// `conversation.get` and live updates shorten the output and long strings
  /// in the input (`crate::activity`); `activity.get` has all of it
  /// (feature `activity_preview`). Older servers leave it out.
  final bool? truncated;

  factory ActivityView.fromJson(Map<String, dynamic> json) => ActivityView(
    callId: (json['call_id'] as String),
    input: json['input'],
    offset: (json['offset'] as num).toInt(),
    ok: (json['ok'] == null ? null : (json['ok'] as bool)),
    output: (json['output'] == null ? null : (json['output'] as String)),
    outputBytes: (json['output_bytes'] == null ? null : (json['output_bytes'] as num).toInt()),
    parentCallId: (json['parent_call_id'] == null ? null : (json['parent_call_id'] as String)),
    tool: (json['tool'] as String),
    truncated: (json['truncated'] == null ? null : (json['truncated'] as bool)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'call_id': callId,
    'input': input,
    'offset': offset,
    if (ok != null) 'ok': ok,
    if (output != null) 'output': output,
    if (outputBytes != null) 'output_bytes': outputBytes,
    if (parentCallId != null) 'parent_call_id': parentCallId,
    'tool': tool,
    if (truncated != null) 'truncated': truncated,
  };
}

/// One piece of work in line (feature `activity_overview`): one per employee and
/// thread, since what waits in one thread becomes one run.
final class ActivityWaiting {
  const ActivityWaiting({
    this.employeeId,
    required this.employeeName,
    required this.sinceMs,
    required this.waitsFor,
    required this.work,
  });

  final String? employeeId;
  final String employeeName;
  final int sinceMs;
  /// slot (its employee is busy), machine (a free slot on this machine), paused,
  /// question, approval, limit, login, later or handoff. Text, not an enum.
  final String waitsFor;
  final ActivityWork work;

  factory ActivityWaiting.fromJson(Map<String, dynamic> json) => ActivityWaiting(
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    employeeName: (json['employee_name'] as String),
    sinceMs: (json['since_ms'] as num).toInt(),
    waitsFor: (json['waits_for'] as String),
    work: ActivityWork.fromJson(json['work'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
    'employee_name': employeeName,
    'since_ms': sinceMs,
    'waits_for': waitsFor,
    'work': work.toJson(),
  };
}

/// What an employee works on or waits with (feature `activity_overview`). With
/// `private` set it is another board member's chat: every other field is empty,
/// so neither its title nor where it is leaves the server.
final class ActivityWork {
  const ActivityWork({
    this.channelId,
    this.chatId,
    this.label = '',
    this.private,
    this.taskId,
    this.ticketId,
  });

  final String? channelId;
  /// A chat of the reader's own with an employee.
  final String? chatId;
  /// "#21 Fix login", a task's title, a channel's name; empty when `private`.
  final String label;
  final bool? private;
  final String? taskId;
  final String? ticketId;

  factory ActivityWork.fromJson(Map<String, dynamic> json) => ActivityWork(
    channelId: (json['channel_id'] == null ? null : (json['channel_id'] as String)),
    chatId: (json['chat_id'] == null ? null : (json['chat_id'] as String)),
    label: (json['label'] == null ? '' : (json['label'] as String)),
    private: (json['private'] == null ? null : (json['private'] as bool)),
    taskId: (json['task_id'] == null ? null : (json['task_id'] as String)),
    ticketId: (json['ticket_id'] == null ? null : (json['ticket_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (channelId != null) 'channel_id': channelId,
    if (chatId != null) 'chat_id': chatId,
    'label': label,
    if (private != null) 'private': private,
    if (taskId != null) 'task_id': taskId,
    if (ticketId != null) 'ticket_id': ticketId,
  };
}

final class AddConnection {
  const AddConnection({
    this.defaultModel,
    required this.displayName,
    required this.provider,
    this.runtimePath,
  });

  final String? defaultModel;
  final String displayName;
  final ProviderKind provider;
  final String? runtimePath;

  factory AddConnection.fromJson(Map<String, dynamic> json) => AddConnection(
    defaultModel: (json['default_model'] == null ? null : (json['default_model'] as String)),
    displayName: (json['display_name'] as String),
    provider: ProviderKind.fromJson(json['provider'] as String),
    runtimePath: (json['runtime_path'] == null ? null : (json['runtime_path'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (defaultModel != null) 'default_model': defaultModel,
    'display_name': displayName,
    'provider': provider.toJson(),
    if (runtimePath != null) 'runtime_path': runtimePath,
  };
}

final class AddIdea {
  const AddIdea({
    this.confirmSecret,
    this.files = const [],
    this.kind,
    this.note,
    required this.projectId,
    required this.title,
  });

  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  /// Files on the server to attach (feature `idea_attachments`): copied into
  /// the project's uploads area, as with a task's `files`.
  final List<String> files;
  final IdeaKind? kind;
  final String? note;
  final String projectId;
  final String title;

  factory AddIdea.fromJson(Map<String, dynamic> json) => AddIdea(
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    files: (json['files'] == null ? const [] : (json['files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    kind: (json['kind'] == null ? null : IdeaKind.fromJson(json['kind'] as String)),
    note: (json['note'] == null ? null : (json['note'] as String)),
    projectId: (json['project_id'] as String),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'files': files,
    if (kind != null) 'kind': kind?.toJson(),
    if (note != null) 'note': note,
    'project_id': projectId,
    'title': title,
  };
}

enum AgentMode {
  /// File and artifact edits in granted folders go ahead; external actions still ask.
  acceptEdits('accept_edits'),
  /// Ask before every change (the default).
  ask('ask'),
  /// Everything goes ahead without asking, except what is always forbidden.
  auto('auto'),
  /// Only read and plan; no changes. Tasks stop after planning until you approve.
  plan('plan');

  const AgentMode(this.wire);

  final String wire;

  static AgentMode fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown AgentMode value: $value'),
      );

  String toJson() => wire;
}

/// One agent at work for an employee.
final class AgentView {
  const AgentView({
    this.callId,
    this.conversationId,
    required this.detail,
    this.doing,
    required this.kind,
    required this.name,
    this.prompt,
    this.runId,
    required this.sinceMs,
    required this.steps,
    this.taskId,
  });

  /// A subagent: the `Task`/`Agent` call that started it, so its steps can be
  /// picked out of `run.get_activity` by `ActivityView::parent_call_id`
  /// (feature `subagent_steps`). Older servers leave it out.
  final String? callId;
  /// The employee's conversation this work shows in (a chat, or the task's).
  final String? conversationId;
  final String detail;
  /// A subagent: its latest step, e.g. "Grep · poll_git_status".
  final String? doing;
  /// conversation | subagent | delegated
  final String kind;
  final String name;
  /// A subagent: what it was asked (shortened).
  final String? prompt;
  final String? runId;
  final int sinceMs;
  /// A subagent: how many tools it used so far.
  final int steps;
  final String? taskId;

  factory AgentView.fromJson(Map<String, dynamic> json) => AgentView(
    callId: (json['call_id'] == null ? null : (json['call_id'] as String)),
    conversationId: (json['conversation_id'] == null ? null : (json['conversation_id'] as String)),
    detail: (json['detail'] as String),
    doing: (json['doing'] == null ? null : (json['doing'] as String)),
    kind: (json['kind'] as String),
    name: (json['name'] as String),
    prompt: (json['prompt'] == null ? null : (json['prompt'] as String)),
    runId: (json['run_id'] == null ? null : (json['run_id'] as String)),
    sinceMs: (json['since_ms'] as num).toInt(),
    steps: (json['steps'] as num).toInt(),
    taskId: (json['task_id'] == null ? null : (json['task_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (callId != null) 'call_id': callId,
    if (conversationId != null) 'conversation_id': conversationId,
    'detail': detail,
    if (doing != null) 'doing': doing,
    'kind': kind,
    'name': name,
    if (prompt != null) 'prompt': prompt,
    if (runId != null) 'run_id': runId,
    'since_ms': sinceMs,
    'steps': steps,
    if (taskId != null) 'task_id': taskId,
  };
}

/// The ideas of every project; each idea carries its own `project_id`.
final class AllIdeasSnapshot {
  const AllIdeasSnapshot({
    required this.ideas,
  });

  final List<IdeaView> ideas;

  factory AllIdeasSnapshot.fromJson(Map<String, dynamic> json) => AllIdeasSnapshot(
    ideas: (json['ideas'] as List<dynamic>).map((x0) => IdeaView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'ideas': ideas.map((x0) => x0.toJson()).toList(),
  };
}

/// The user's answers to a question a runtime asked (Claude Code's
/// AskUserQuestion), keyed by the question text: the chosen option label,
/// several labels joined by ", ", or the user's own words.
final class AnswerQuestion {
  const AnswerQuestion({
    required this.answers,
    required this.approvalId,
    this.confirmSecret,
  });

  final Map<String, String> answers;
  final String approvalId;
  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;

  factory AnswerQuestion.fromJson(Map<String, dynamic> json) => AnswerQuestion(
    answers: (json['answers'] as Map<String, dynamic>).map((k0, v0) => MapEntry(k0, (v0 as String))),
    approvalId: (json['approval_id'] as String),
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'answers': answers,
    'approval_id': approvalId,
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
  };
}

final class AppError {
  const AppError({
    required this.code,
    this.matchedItems = const [],
    required this.message,
    this.retryAfterMs,
    this.tooLarge,
  });

  final ErrorCode code;
  /// `looks_like_secret`: the names of the vault items the text holds the value of.
  final List<String> matchedItems;
  /// User-facing explanation.
  final String message;
  /// `too_many_attempts`: how long until the next try is taken.
  final int? retryAfterMs;
  /// `guideline_too_large`: the characters, the limit and the combination that overflows.
  final GuidelineTooLarge? tooLarge;

  factory AppError.fromJson(Map<String, dynamic> json) => AppError(
    code: ErrorCode.fromJson(json['code'] as String),
    matchedItems: (json['matched_items'] == null ? const [] : (json['matched_items'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    message: (json['message'] as String),
    retryAfterMs: (json['retry_after_ms'] == null ? null : (json['retry_after_ms'] as num).toInt()),
    tooLarge: (json['too_large'] == null ? null : GuidelineTooLarge.fromJson(json['too_large'] as Map<String, dynamic>)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'code': code.toJson(),
    'matched_items': matchedItems,
    'message': message,
    if (retryAfterMs != null) 'retry_after_ms': retryAfterMs,
    if (tooLarge != null) 'too_large': tooLarge?.toJson(),
  };
}

enum ApprovalStatus {
  approved('approved'),
  consumed('consumed'),
  expired('expired'),
  invalidated('invalidated'),
  pending('pending'),
  rejected('rejected');

  const ApprovalStatus(this.wire);

  final String wire;

  static ApprovalStatus fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown ApprovalStatus value: $value'),
      );

  String toJson() => wire;
}

/// Everything the user needs to judge one exact action.
final class ApprovalView {
  const ApprovalView({
    required this.account,
    required this.actionDigest,
    this.alwaysAllowEligible = false,
    required this.arguments,
    this.conversationId,
    this.discussionId,
    required this.effect,
    required this.expiresAtMs,
    required this.id,
    required this.reasons,
    required this.requestedAtMs,
    required this.requester,
    required this.resources,
    required this.runId,
    required this.status,
    this.taskId,
    this.ticketId,
    required this.tool,
  });

  final String account;
  final String actionDigest;
  /// Whether clients may offer 'Always allow' (an MCP tool of the user's
  /// servers or Claude in Chrome); answered with `DecideApproval::always_allow`.
  final bool alwaysAllowEligible;
  final dynamic arguments;
  final String? conversationId;
  /// The discussion (a channel, a ticket's) that woke the run (feature `run_cards`).
  /// Left out for other runs and by older servers.
  final String? discussionId;
  final String effect;
  final int expiresAtMs;
  final String id;
  final List<String> reasons;
  final int requestedAtMs;
  /// Who asked: the employee's name, or "Chat".
  final String requester;
  final List<String> resources;
  final String runId;
  final ApprovalStatus status;
  /// The task, when a task's conversation asked.
  final String? taskId;
  /// The ticket it waits on: of the run's task, or of the discussion that woke
  /// the run (feature `run_cards`). Left out for other runs and by older servers.
  final String? ticketId;
  final String tool;

  factory ApprovalView.fromJson(Map<String, dynamic> json) => ApprovalView(
    account: (json['account'] as String),
    actionDigest: (json['action_digest'] as String),
    alwaysAllowEligible: (json['always_allow_eligible'] == null ? false : (json['always_allow_eligible'] as bool)),
    arguments: json['arguments'],
    conversationId: (json['conversation_id'] == null ? null : (json['conversation_id'] as String)),
    discussionId: (json['discussion_id'] == null ? null : (json['discussion_id'] as String)),
    effect: (json['effect'] as String),
    expiresAtMs: (json['expires_at_ms'] as num).toInt(),
    id: (json['id'] as String),
    reasons: (json['reasons'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    requestedAtMs: (json['requested_at_ms'] as num).toInt(),
    requester: (json['requester'] as String),
    resources: (json['resources'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    runId: (json['run_id'] as String),
    status: ApprovalStatus.fromJson(json['status'] as String),
    taskId: (json['task_id'] == null ? null : (json['task_id'] as String)),
    ticketId: (json['ticket_id'] == null ? null : (json['ticket_id'] as String)),
    tool: (json['tool'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'account': account,
    'action_digest': actionDigest,
    'always_allow_eligible': alwaysAllowEligible,
    'arguments': arguments,
    if (conversationId != null) 'conversation_id': conversationId,
    if (discussionId != null) 'discussion_id': discussionId,
    'effect': effect,
    'expires_at_ms': expiresAtMs,
    'id': id,
    'reasons': reasons,
    'requested_at_ms': requestedAtMs,
    'requester': requester,
    'resources': resources,
    'run_id': runId,
    'status': status.toJson(),
    if (taskId != null) 'task_id': taskId,
    if (ticketId != null) 'ticket_id': ticketId,
    'tool': tool,
  };
}

final class ApprovalsSnapshot {
  const ApprovalsSnapshot({
    required this.approvals,
    required this.asOfSeq,
  });

  final List<ApprovalView> approvals;
  final int asOfSeq;

  factory ApprovalsSnapshot.fromJson(Map<String, dynamic> json) => ApprovalsSnapshot(
    approvals: (json['approvals'] as List<dynamic>).map((x0) => ApprovalView.fromJson(x0 as Map<String, dynamic>)).toList(),
    asOfSeq: (json['as_of_seq'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'approvals': approvals.map((x0) => x0.toJson()).toList(),
    'as_of_seq': asOfSeq,
  };
}

/// An HTML file an employee is writing, before its `Write` call is complete: the
/// app shows it as it grows, as Claude Desktop does with an artifact. Display
/// only, never stored; the call itself arrives in `activity` once it is whole.
final class ArtifactDraft {
  const ArtifactDraft({
    required this.callId,
    required this.content,
    required this.path,
  });

  /// The `Write` call's id (`ActivityView::call_id` once it is whole).
  final String callId;
  /// The content written so far (at most `ARTIFACT_DRAFT_MAX_BYTES`).
  final String content;
  /// The file's path as the employee wrote it.
  final String path;

  factory ArtifactDraft.fromJson(Map<String, dynamic> json) => ArtifactDraft(
    callId: (json['call_id'] as String),
    content: (json['content'] as String),
    path: (json['path'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'call_id': callId,
    'content': content,
    'path': path,
  };
}

/// The user gives a task to an employee (or takes it back: a draft). Allowed
/// while nothing runs: draft, queued or waiting.
final class AssignTask {
  const AssignTask({
    this.employeeId,
    required this.taskId,
  });

  final String? employeeId;
  final String taskId;

  factory AssignTask.fromJson(Map<String, dynamic> json) => AssignTask(
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    taskId: (json['task_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
    'task_id': taskId,
  };
}

/// The board member who started a session chooses who takes it, while the
/// dispatcher still chooses or after it chose nobody (feature `session_dispatch`).
/// The chosen employee answers the first message at once.
final class AssignWorkSession {
  const AssignWorkSession({
    required this.conversationId,
    required this.employeeId,
  });

  final String conversationId;
  final String employeeId;

  factory AssignWorkSession.fromJson(Map<String, dynamic> json) => AssignWorkSession(
    conversationId: (json['conversation_id'] as String),
    employeeId: (json['employee_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversation_id': conversationId,
    'employee_id': employeeId,
  };
}

/// Snapshot a user-picked file into a conversation.
final class AttachFile {
  const AttachFile({
    required this.conversationId,
    required this.path,
  });

  final String conversationId;
  final String path;

  factory AttachFile.fromJson(Map<String, dynamic> json) => AttachFile(
    conversationId: (json['conversation_id'] as String),
    path: (json['path'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversation_id': conversationId,
    'path': path,
  };
}

final class AttachmentView {
  const AttachmentView({
    required this.byteLength,
    required this.id,
    required this.kind,
    required this.name,
    required this.truncated,
  });

  final int byteLength;
  final String id;
  final String kind;
  final String name;
  final bool truncated;

  factory AttachmentView.fromJson(Map<String, dynamic> json) => AttachmentView(
    byteLength: (json['byte_length'] as num).toInt(),
    id: (json['id'] as String),
    kind: (json['kind'] as String),
    name: (json['name'] as String),
    truncated: (json['truncated'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'byte_length': byteLength,
    'id': id,
    'kind': kind,
    'name': name,
    'truncated': truncated,
  };
}

/// `auto_allow.get_setting` and `auto_allow.set_enabled`: whether the safe read-only tools
/// (`tools`) pass without a question in every mode but Plan. On by default.
final class AutoAllowSetting {
  const AutoAllowSetting({
    required this.enabled,
    this.tools = const [],
  });

  final bool enabled;
  /// The tools the setting covers, for display; fixed by the server.
  final List<String> tools;

  factory AutoAllowSetting.fromJson(Map<String, dynamic> json) => AutoAllowSetting(
    enabled: (json['enabled'] as bool),
    tools: (json['tools'] == null ? const [] : (json['tools'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'enabled': enabled,
    'tools': tools,
  };
}

final class BackupView {
  const BackupView({
    required this.appVersion,
    required this.bytes,
    required this.createdAtMs,
    required this.files,
    required this.id,
    required this.kind,
    required this.scrubbed,
  });

  final String appVersion;
  final int bytes;
  final int createdAtMs;
  final int files;
  final String id;
  /// "auto" or "manual"
  final String kind;
  /// Forgotten content was removed from this backup afterwards.
  final bool scrubbed;

  factory BackupView.fromJson(Map<String, dynamic> json) => BackupView(
    appVersion: (json['app_version'] as String),
    bytes: (json['bytes'] as num).toInt(),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    files: (json['files'] as num).toInt(),
    id: (json['id'] as String),
    kind: (json['kind'] as String),
    scrubbed: (json['scrubbed'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'app_version': appVersion,
    'bytes': bytes,
    'created_at_ms': createdAtMs,
    'files': files,
    'id': id,
    'kind': kind,
    'scrubbed': scrubbed,
  };
}

final class BackupsSnapshot {
  const BackupsSnapshot({
    required this.backups,
    this.error,
    required this.folder,
    this.paused,
    this.restorePending,
  });

  final List<BackupView> backups;
  final String? error;
  final String folder;
  /// Set after a restore: background work waits until resumed.
  final String? paused;
  final String? restorePending;

  factory BackupsSnapshot.fromJson(Map<String, dynamic> json) => BackupsSnapshot(
    backups: (json['backups'] as List<dynamic>).map((x0) => BackupView.fromJson(x0 as Map<String, dynamic>)).toList(),
    error: (json['error'] == null ? null : (json['error'] as String)),
    folder: (json['folder'] as String),
    paused: (json['paused'] == null ? null : (json['paused'] as String)),
    restorePending: (json['restore_pending'] == null ? null : (json['restore_pending'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'backups': backups.map((x0) => x0.toJson()).toList(),
    if (error != null) 'error': error,
    'folder': folder,
    if (paused != null) 'paused': paused,
    if (restorePending != null) 'restore_pending': restorePending,
  };
}

/// One column with its cards, in order.
final class BoardColumnCards {
  const BoardColumnCards({
    required this.column,
    required this.tickets,
  });

  final BoardColumnView column;
  final List<TicketView> tickets;

  factory BoardColumnCards.fromJson(Map<String, dynamic> json) => BoardColumnCards(
    column: BoardColumnView.fromJson(json['column'] as Map<String, dynamic>),
    tickets: (json['tickets'] as List<dynamic>).map((x0) => TicketView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'column': column.toJson(),
    'tickets': tickets.map((x0) => x0.toJson()).toList(),
  };
}

final class BoardColumnEdit {
  const BoardColumnEdit({
    required this.columnId,
    required this.name,
  });

  final String columnId;
  final String name;

  factory BoardColumnEdit.fromJson(Map<String, dynamic> json) => BoardColumnEdit(
    columnId: (json['column_id'] as String),
    name: (json['name'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'column_id': columnId,
    'name': name,
  };
}

final class BoardColumnView {
  const BoardColumnView({
    required this.id,
    required this.kind,
    required this.name,
    required this.position,
    required this.version,
  });

  final String id;
  final ColumnKind kind;
  final String name;
  /// 0 is the leftmost column.
  final int position;
  final int version;

  factory BoardColumnView.fromJson(Map<String, dynamic> json) => BoardColumnView(
    id: (json['id'] as String),
    kind: ColumnKind.fromJson(json['kind'] as String),
    name: (json['name'] as String),
    position: (json['position'] as num).toInt(),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'id': id,
    'kind': kind.toJson(),
    'name': name,
    'position': position,
    'version': version,
  };
}

/// `board.get`: the seven columns left to right, each with its cards (not archived).
final class BoardView {
  const BoardView({
    required this.asOfSeq,
    required this.columns,
    this.projectId,
  });

  final int asOfSeq;
  final List<BoardColumnCards> columns;
  /// The project filter it was read with; none: every project (B4).
  final String? projectId;

  factory BoardView.fromJson(Map<String, dynamic> json) => BoardView(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    columns: (json['columns'] as List<dynamic>).map((x0) => BoardColumnCards.fromJson(x0 as Map<String, dynamic>)).toList(),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'columns': columns.map((x0) => x0.toJson()).toList(),
    if (projectId != null) 'project_id': projectId,
  };
}

/// How a run drives the browser when the user allowed it for an employee.
enum BrowserBackend {
  /// The Claude in Chrome extension in the user's own browser profile.
  claudeInChrome('claude_in_chrome'),
  /// Playwright MCP attached to a browser window of its own (an Aurelius
  /// profile in Chrome or Edge, kept between runs, so logins stay). Its tools
  /// follow the mode like other MCP tools and parallel runs each get a tab.
  playwright('playwright');

  const BrowserBackend(this.wire);

  final String wire;

  static BrowserBackend fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown BrowserBackend value: $value'),
      );

  String toJson() => wire;
}

/// The browser that carries the Claude in Chrome extension.
enum BrowserChoice {
  /// Whichever is running, else the one that has the extension.
  automatic('automatic'),
  chrome('chrome'),
  edge('edge');

  const BrowserChoice(this.wire);

  final String wire;

  static BrowserChoice fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown BrowserChoice value: $value'),
      );

  String toJson() => wire;
}

/// `browser.get` and `browser.set`.
final class BrowserSetting {
  const BrowserSetting({
    this.backend,
    required this.choice,
    this.envOverride,
  });

  /// How runs drive the browser. Always sent by servers that know the choice;
  /// absent from older servers, which only have Claude in Chrome.
  final BrowserBackend? backend;
  /// What the user chose.
  final BrowserChoice choice;
  /// `AURELIUS_BROWSER` is set on the server and wins over `choice`.
  final BrowserChoice? envOverride;

  factory BrowserSetting.fromJson(Map<String, dynamic> json) => BrowserSetting(
    backend: (json['backend'] == null ? null : BrowserBackend.fromJson(json['backend'] as String)),
    choice: BrowserChoice.fromJson(json['choice'] as String),
    envOverride: (json['env_override'] == null ? null : BrowserChoice.fromJson(json['env_override'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (backend != null) 'backend': backend?.toJson(),
    'choice': choice.toJson(),
    if (envOverride != null) 'env_override': envOverride?.toJson(),
  };
}

/// `scheduled_message.cancel`: a message an employee scheduled does not go out.
final class CancelScheduledMessage {
  const CancelScheduledMessage({
    required this.messageId,
  });

  final String messageId;

  factory CancelScheduledMessage.fromJson(Map<String, dynamic> json) => CancelScheduledMessage(
    messageId: (json['message_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'message_id': messageId,
  };
}

final class CancelTask {
  const CancelTask({
    required this.taskId,
  });

  final String taskId;

  factory CancelTask.fromJson(Map<String, dynamic> json) => CancelTask(
    taskId: (json['task_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'task_id': taskId,
  };
}

enum Capability {
  /// Decide concrete approvals; switch a conversation to Auto.
  approve('approve'),
  /// Chats and project conversations: send, retry, stop, attach, share.
  chat('chat'),
  /// A paired computer with full access: connections and keys, folders on
  /// the server, backups, updates and remote access itself.
  owner('owner'),
  /// Every query and the event stream.
  read('read'),
  /// Projects, employees, tasks, plans and memory.
  taskControl('task_control');

  const Capability(this.wire);

  final String wire;

  static Capability fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown Capability value: $value'),
      );

  String toJson() => wire;
}

/// What a channel is (`ChannelView::kind`, feature `ticket_channels`).
enum ChannelKind {
  /// The general channel (no project).
  general('general'),
  /// A project's channel.
  project('project'),
  /// A ticket's channel: its discussion.
  ticket('ticket'),
  /// A kind of a newer server this client does not know.
  unknown('unknown');

  const ChannelKind(this.wire);

  final String wire;

  static ChannelKind fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => ChannelKind.unknown,
      );

  String toJson() => wire;
}

/// A channel of the Team chat (`channel.list`, feature `team_channels`): the
/// general one (no project), one per project, and (feature `ticket_channels`,
/// only for a connection that accepts it) one per ticket. A closed ticket's
/// channel (ticket on Done or archived) reports `unread` and `mentions` 0, so old
/// closed tickets never light up the Team badge; an archived project's channel
/// still counts.
final class ChannelView {
  const ChannelView({
    required this.archived,
    required this.conversationId,
    this.kind,
    this.lastActivityMs,
    this.member,
    this.memberCount,
    required this.mentions,
    required this.name,
    this.projectId,
    this.readThrough,
    this.ticketColumn,
    this.ticketId,
    this.ticketNumber,
    required this.unread,
  });

  /// The channel can only be read: its project is archived, or (a ticket's
  /// channel) the ticket is on Done or archived.
  final bool archived;
  final String conversationId;
  /// What the channel is (feature `ticket_channels`); left out for General and by older servers.
  final ChannelKind? kind;
  final int? lastActivityMs;
  /// The board member who reads the list is a member of the channel (a
  /// ticket's channel: its creator, owner, the Owner, or who joined later).
  final bool? member;
  /// How many board members and employees are members (a ticket's channel).
  final int? memberCount;
  /// Of those, the ones that name a board member with `@Name` (B8: the badge counts these).
  final int mentions;
  /// "General" or the project's name.
  final String name;
  /// None: the general channel.
  final String? projectId;
  /// The highest message ordinal the reader had read before this read (the
  /// line "New messages" goes after it); none: never read.
  final int? readThrough;
  /// A ticket's channel: the kind of the column the ticket is in.
  final ColumnKind? ticketColumn;
  /// A ticket's channel: the ticket.
  final String? ticketId;
  /// A ticket's channel: the ticket's number (`#N`).
  final int? ticketNumber;
  /// Messages since the last one read; 0 for a closed ticket's channel.
  final int unread;

  factory ChannelView.fromJson(Map<String, dynamic> json) => ChannelView(
    archived: (json['archived'] as bool),
    conversationId: (json['conversation_id'] as String),
    kind: (json['kind'] == null ? null : ChannelKind.fromJson(json['kind'] as String)),
    lastActivityMs: (json['last_activity_ms'] == null ? null : (json['last_activity_ms'] as num).toInt()),
    member: (json['member'] == null ? null : (json['member'] as bool)),
    memberCount: (json['member_count'] == null ? null : (json['member_count'] as num).toInt()),
    mentions: (json['mentions'] as num).toInt(),
    name: (json['name'] as String),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
    readThrough: (json['read_through'] == null ? null : (json['read_through'] as num).toInt()),
    ticketColumn: (json['ticket_column'] == null ? null : ColumnKind.fromJson(json['ticket_column'] as String)),
    ticketId: (json['ticket_id'] == null ? null : (json['ticket_id'] as String)),
    ticketNumber: (json['ticket_number'] == null ? null : (json['ticket_number'] as num).toInt()),
    unread: (json['unread'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'archived': archived,
    'conversation_id': conversationId,
    if (kind != null) 'kind': kind?.toJson(),
    if (lastActivityMs != null) 'last_activity_ms': lastActivityMs,
    if (member != null) 'member': member,
    if (memberCount != null) 'member_count': memberCount,
    'mentions': mentions,
    'name': name,
    if (projectId != null) 'project_id': projectId,
    if (readThrough != null) 'read_through': readThrough,
    if (ticketColumn != null) 'ticket_column': ticketColumn?.toJson(),
    if (ticketId != null) 'ticket_id': ticketId,
    if (ticketNumber != null) 'ticket_number': ticketNumber,
    'unread': unread,
  };
}

/// `channel.list`: the general channel first, then one per project by name.
final class ChannelsSnapshot {
  const ChannelsSnapshot({
    required this.asOfSeq,
    required this.channels,
  });

  final int asOfSeq;
  final List<ChannelView> channels;

  factory ChannelsSnapshot.fromJson(Map<String, dynamic> json) => ChannelsSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    channels: (json['channels'] as List<dynamic>).map((x0) => ChannelView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'channels': channels.map((x0) => x0.toJson()).toList(),
  };
}

/// `claude_login.input`. The text (a one-time code) is never printed.
final class ClaudeLoginInputParams {
  const ClaudeLoginInputParams({
    required this.linkId,
    required this.text,
  });

  final String linkId;
  final String text;

  factory ClaudeLoginInputParams.fromJson(Map<String, dynamic> json) => ClaudeLoginInputParams(
    linkId: (json['link_id'] as String),
    text: (json['text'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'link_id': linkId,
    'text': text,
  };
}

/// `ServerMessage::ClaudeLoginPrompt`: the sign-in link to open and/or the
/// question for the code, as `claude` printed them.
final class ClaudeLoginPrompt {
  const ClaudeLoginPrompt({
    required this.asksCode,
    this.error,
    required this.linkId,
    this.url,
  });

  /// Claude waits for the code: send it with `claude_login.input`.
  final bool asksCode;
  /// The link ended without a login: a fixed text of Aurelius (never Claude's
  /// output), shown with "Try again". A link that worked ends with the event
  /// `claude_login.updated` instead.
  final String? error;
  final String linkId;
  /// The Anthropic sign-in page to open (`https://claude.com/…/oauth/authorize?…`).
  final String? url;

  factory ClaudeLoginPrompt.fromJson(Map<String, dynamic> json) => ClaudeLoginPrompt(
    asksCode: (json['asks_code'] as bool),
    error: (json['error'] == null ? null : (json['error'] as String)),
    linkId: (json['link_id'] as String),
    url: (json['url'] == null ? null : (json['url'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'asks_code': asksCode,
    if (error != null) 'error': error,
    'link_id': linkId,
    if (url != null) 'url': url,
  };
}

/// The state of a board member's own Claude login (`<data>/logins/<user_id>/`).
enum ClaudeLoginStatus {
  /// Claude asked to sign in again (a 401 in a run): Reconnect.
  expired('expired'),
  linked('linked'),
  /// Never connected, or disconnected: the board member's tickets wait (`needs: login`).
  missing('missing');

  const ClaudeLoginStatus(this.wire);

  final String wire;

  static ClaudeLoginStatus fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown ClaudeLoginStatus value: $value'),
      );

  String toJson() => wire;
}

/// `claude_login.status`, `HelloResult::claude_login`: the session's own login.
/// Never a token or a path; the email and the plan come from `claude auth status`.
final class ClaudeLoginView {
  const ClaudeLoginView({
    required this.background,
    this.checkedAtMs,
    this.email,
    this.linkId,
    this.linkedAtMs,
    this.plan,
    required this.status,
    required this.userId,
  });

  /// This login also runs the company's background jobs (memory
  /// consolidation, B6): `company.set_background_login`.
  final bool background;
  final int? checkedAtMs;
  final String? email;
  /// A `claude_login.link` of this board member is running (one at a time).
  final String? linkId;
  final int? linkedAtMs;
  /// e.g. "max", "pro"; none when unknown.
  final String? plan;
  final ClaudeLoginStatus status;
  final String userId;

  factory ClaudeLoginView.fromJson(Map<String, dynamic> json) => ClaudeLoginView(
    background: (json['background'] as bool),
    checkedAtMs: (json['checked_at_ms'] == null ? null : (json['checked_at_ms'] as num).toInt()),
    email: (json['email'] == null ? null : (json['email'] as String)),
    linkId: (json['link_id'] == null ? null : (json['link_id'] as String)),
    linkedAtMs: (json['linked_at_ms'] == null ? null : (json['linked_at_ms'] as num).toInt()),
    plan: (json['plan'] == null ? null : (json['plan'] as String)),
    status: ClaudeLoginStatus.fromJson(json['status'] as String),
    userId: (json['user_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'background': background,
    if (checkedAtMs != null) 'checked_at_ms': checkedAtMs,
    if (email != null) 'email': email,
    if (linkId != null) 'link_id': linkId,
    if (linkedAtMs != null) 'linked_at_ms': linkedAtMs,
    if (plan != null) 'plan': plan,
    'status': status.toJson(),
    'user_id': userId,
  };
}

/// One local branch.
final class CodeBranch {
  const CodeBranch({
    required this.ahead,
    required this.atMs,
    required this.author,
    required this.behind,
    this.commits = const [],
    this.deletions = 0,
    this.filesChanged = 0,
    required this.head,
    this.insertions = 0,
    this.isMain = false,
    this.merged = false,
    required this.name,
    this.open = false,
    this.owner,
    required this.subject,
    this.ticket,
    this.upstream,
    this.upstreamAhead = 0,
    this.upstreamBehind = 0,
    this.working = false,
    this.worktreePath,
  });

  /// Commits on this branch that main lacks (`main..branch`); 0 for main.
  final int ahead;
  final int atMs;
  final String author;
  /// Commits on main this branch lacks (`branch..main`); 0 for main.
  final int behind;
  /// Its own commits (`main..branch`), newest first, at most `CODE_BRANCH_COMMITS_MAX`.
  final List<CodeCommitLine> commits;
  final int deletions;
  /// The size of `main...branch` (what a merge would bring): files, lines added, removed.
  final int filesChanged;
  /// Its newest commit.
  final String head;
  final int insertions;
  /// The repository's main branch.
  final bool isMain;
  /// Every commit of it is in main (git's view: a fresh branch at main counts too; see `open`).
  final bool merged;
  final String name;
  /// Its ticket is not on Done (nor archived) or its work session is not deleted: work on
  /// it may still come, also when nothing is ahead of main yet. The Code tab never offers to
  /// remove or delete such a branch, and Clean up merged leaves it alone.
  final bool open;
  /// Who works on it: the ticket's owner, else the employee of the work session.
  final CodeOwner? owner;
  /// Its newest commit: subject, author name and time.
  final String subject;
  /// The ticket it belongs to: a quick task's branch, a work session's worktree, or a
  /// ticket number in its name (`feat/ticket-77-…`, `fix/77-…`) of this project.
  final CodeTicket? ticket;
  /// Its upstream (`origin/…`); for main: how far it is from it after a fetch.
  final String? upstream;
  final int upstreamAhead;
  final int upstreamBehind;
  /// An employee works on it now: a run of its ticket or work session is live or queued.
  final bool working;
  /// The worktree it is checked out in; none: no worktree.
  final String? worktreePath;

  factory CodeBranch.fromJson(Map<String, dynamic> json) => CodeBranch(
    ahead: (json['ahead'] as num).toInt(),
    atMs: (json['at_ms'] as num).toInt(),
    author: (json['author'] as String),
    behind: (json['behind'] as num).toInt(),
    commits: (json['commits'] == null ? const [] : (json['commits'] as List<dynamic>).map((x0) => CodeCommitLine.fromJson(x0 as Map<String, dynamic>)).toList()),
    deletions: (json['deletions'] == null ? 0 : (json['deletions'] as num).toInt()),
    filesChanged: (json['files_changed'] == null ? 0 : (json['files_changed'] as num).toInt()),
    head: (json['head'] as String),
    insertions: (json['insertions'] == null ? 0 : (json['insertions'] as num).toInt()),
    isMain: (json['is_main'] == null ? false : (json['is_main'] as bool)),
    merged: (json['merged'] == null ? false : (json['merged'] as bool)),
    name: (json['name'] as String),
    open: (json['open'] == null ? false : (json['open'] as bool)),
    owner: (json['owner'] == null ? null : CodeOwner.fromJson(json['owner'] as Map<String, dynamic>)),
    subject: (json['subject'] as String),
    ticket: (json['ticket'] == null ? null : CodeTicket.fromJson(json['ticket'] as Map<String, dynamic>)),
    upstream: (json['upstream'] == null ? null : (json['upstream'] as String)),
    upstreamAhead: (json['upstream_ahead'] == null ? 0 : (json['upstream_ahead'] as num).toInt()),
    upstreamBehind: (json['upstream_behind'] == null ? 0 : (json['upstream_behind'] as num).toInt()),
    working: (json['working'] == null ? false : (json['working'] as bool)),
    worktreePath: (json['worktree_path'] == null ? null : (json['worktree_path'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'ahead': ahead,
    'at_ms': atMs,
    'author': author,
    'behind': behind,
    'commits': commits.map((x0) => x0.toJson()).toList(),
    'deletions': deletions,
    'files_changed': filesChanged,
    'head': head,
    'insertions': insertions,
    'is_main': isMain,
    'merged': merged,
    'name': name,
    'open': open,
    if (owner != null) 'owner': owner?.toJson(),
    'subject': subject,
    if (ticket != null) 'ticket': ticket?.toJson(),
    if (upstream != null) 'upstream': upstream,
    'upstream_ahead': upstreamAhead,
    'upstream_behind': upstreamBehind,
    'working': working,
    if (worktreePath != null) 'worktree_path': worktreePath,
  };
}

/// One commit of the graph.
final class CodeCommit {
  const CodeCommit({
    required this.atMs,
    required this.author,
    this.branches = const [],
    required this.hash,
    this.parents = const [],
    required this.subject,
  });

  final int atMs;
  final String author;
  /// The listed local branches whose newest commit this is.
  final List<String> branches;
  /// The full hash; the client shows the first seven characters.
  final String hash;
  /// Its parents' full hashes, the first parent first (two or more: a merge).
  final List<String> parents;
  final String subject;

  factory CodeCommit.fromJson(Map<String, dynamic> json) => CodeCommit(
    atMs: (json['at_ms'] as num).toInt(),
    author: (json['author'] as String),
    branches: (json['branches'] == null ? const [] : (json['branches'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    hash: (json['hash'] as String),
    parents: (json['parents'] == null ? const [] : (json['parents'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    subject: (json['subject'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'at_ms': atMs,
    'author': author,
    'branches': branches,
    'hash': hash,
    'parents': parents,
    'subject': subject,
  };
}

/// A commit of a branch's own list.
final class CodeCommitLine {
  const CodeCommitLine({
    required this.hash,
    required this.subject,
  });

  final String hash;
  final String subject;

  factory CodeCommitLine.fromJson(Map<String, dynamic> json) => CodeCommitLine(
    hash: (json['hash'] as String),
    subject: (json['subject'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'hash': hash,
    'subject': subject,
  };
}

/// Who works on a branch.
final class CodeOwner {
  const CodeOwner({
    required this.employeeId,
    required this.name,
  });

  final String employeeId;
  final String name;

  factory CodeOwner.fromJson(Map<String, dynamic> json) => CodeOwner(
    employeeId: (json['employee_id'] as String),
    name: (json['name'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'employee_id': employeeId,
    'name': name,
  };
}

/// The ticket a branch belongs to.
final class CodeTicket {
  const CodeTicket({
    required this.number,
    required this.ticketId,
    required this.title,
  });

  final int number;
  final String ticketId;
  final String title;

  factory CodeTicket.fromJson(Map<String, dynamic> json) => CodeTicket(
    number: (json['number'] as num).toInt(),
    ticketId: (json['ticket_id'] as String),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'number': number,
    'ticket_id': ticketId,
    'title': title,
  };
}

/// The git state of one project's folder.
final class CodeView {
  const CodeView({
    this.branches = const [],
    this.commits = const [],
    this.fetchedAtMs,
    this.hasRemote = false,
    this.mainBranch,
    this.problem,
    required this.projectId,
    required this.repoPath,
    this.worktrees = const [],
  });

  /// The local branches, main first, then the newest last commit first.
  final List<CodeBranch> branches;
  /// The newest commits of main and every listed branch, newest first (date order),
  /// at most `CODE_COMMITS_MAX`. A parent outside the list is cut off in the graph.
  final List<CodeCommit> commits;
  /// When the last `git fetch` ended (the time of `FETCH_HEAD`); none: never.
  final int? fetchedAtMs;
  /// Whether the repository has a remote at all (no remote: no Fetch).
  final bool hasRemote;
  /// The repository's main branch (`main`, else `master`); none: see `problem`.
  final String? mainBranch;
  /// Why there is nothing to show, in words a board member can act on (no folder,
  /// not a git repository, git missing, no main branch). The lists are empty then.
  final String? problem;
  final String projectId;
  /// The project's folder (the main checkout).
  final String repoPath;
  /// Every worktree git knows, the main checkout first.
  final List<CodeWorktree> worktrees;

  factory CodeView.fromJson(Map<String, dynamic> json) => CodeView(
    branches: (json['branches'] == null ? const [] : (json['branches'] as List<dynamic>).map((x0) => CodeBranch.fromJson(x0 as Map<String, dynamic>)).toList()),
    commits: (json['commits'] == null ? const [] : (json['commits'] as List<dynamic>).map((x0) => CodeCommit.fromJson(x0 as Map<String, dynamic>)).toList()),
    fetchedAtMs: (json['fetched_at_ms'] == null ? null : (json['fetched_at_ms'] as num).toInt()),
    hasRemote: (json['has_remote'] == null ? false : (json['has_remote'] as bool)),
    mainBranch: (json['main_branch'] == null ? null : (json['main_branch'] as String)),
    problem: (json['problem'] == null ? null : (json['problem'] as String)),
    projectId: (json['project_id'] as String),
    repoPath: (json['repo_path'] as String),
    worktrees: (json['worktrees'] == null ? const [] : (json['worktrees'] as List<dynamic>).map((x0) => CodeWorktree.fromJson(x0 as Map<String, dynamic>)).toList()),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'branches': branches.map((x0) => x0.toJson()).toList(),
    'commits': commits.map((x0) => x0.toJson()).toList(),
    if (fetchedAtMs != null) 'fetched_at_ms': fetchedAtMs,
    'has_remote': hasRemote,
    if (mainBranch != null) 'main_branch': mainBranch,
    if (problem != null) 'problem': problem,
    'project_id': projectId,
    'repo_path': repoPath,
    'worktrees': worktrees.map((x0) => x0.toJson()).toList(),
  };
}

/// One worktree of the repository.
final class CodeWorktree {
  const CodeWorktree({
    this.branch,
    this.changedCount = 0,
    this.changedFiles = const [],
    required this.head,
    this.isMain = false,
    this.missing = false,
    required this.path,
    this.working = false,
  });

  /// Its branch; none: a detached HEAD.
  final String? branch;
  /// How many files have uncommitted changes (may be more than `changed_files` names).
  final int changedCount;
  /// Uncommitted changes (tracked and untracked files), at most `CODE_FILES_MAX` named.
  final List<String> changedFiles;
  /// The commit it is on.
  final String head;
  /// The project's folder itself; never removed from here.
  final bool isMain;
  /// Git no longer finds its folder (`git worktree prune` removes it).
  final bool missing;
  final String path;
  /// An employee works in it now: its branch is `working`, or (a detached HEAD) a run of
  /// the work session in this folder is live or queued. It is not removed then.
  final bool working;

  factory CodeWorktree.fromJson(Map<String, dynamic> json) => CodeWorktree(
    branch: (json['branch'] == null ? null : (json['branch'] as String)),
    changedCount: (json['changed_count'] == null ? 0 : (json['changed_count'] as num).toInt()),
    changedFiles: (json['changed_files'] == null ? const [] : (json['changed_files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    head: (json['head'] as String),
    isMain: (json['is_main'] == null ? false : (json['is_main'] as bool)),
    missing: (json['missing'] == null ? false : (json['missing'] as bool)),
    path: (json['path'] as String),
    working: (json['working'] == null ? false : (json['working'] as bool)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (branch != null) 'branch': branch,
    'changed_count': changedCount,
    'changed_files': changedFiles,
    'head': head,
    'is_main': isMain,
    'missing': missing,
    'path': path,
    'working': working,
  };
}

/// What a column does. The seven kinds always exist, once each.
enum ColumnKind {
  /// New tickets; nothing starts from here by itself.
  backlog('backlog'),
  /// The main task runs or waits (for a colleague, an approval, input).
  doing('doing'),
  /// Accepted by a board member; read-only.
  done('done'),
  /// A plan waits for a board member (Plan mode only).
  planReview('plan_review'),
  /// The main task reported; a board member accepts it or sends it back.
  review('review'),
  /// The main task reported; the tester runs the full test suite once before
  /// Review (protocol 5, feature `board_testing`). Only with a company tester:
  /// without one, cards go from Doing to Review as before.
  testing('testing'),
  /// Started: waiting for the dispatcher or for the owner's turn.
  todo('todo');

  const ColumnKind(this.wire);

  final String wire;

  static ColumnKind fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown ColumnKind value: $value'),
      );

  String toJson() => wire;
}

/// Every business mutation of the domain enters the system as one of these
/// commands. `Command::method` gives its protocol method name.
sealed class Command {
  const Command();

  factory Command.fromJson(Map<String, dynamic> json) {
    final tag = json['type'] as String;
    switch (tag) {
      case 'accept_ticket':
        return CommandAcceptTicket(
          confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
          note: (json['note'] == null ? null : (json['note'] as String)),
          ticketId: (json['ticket_id'] as String),
        );
      case 'add_connection':
        return CommandAddConnection(value: AddConnection.fromJson(json as Map<String, dynamic>));
      case 'add_idea':
        return CommandAddIdea(value: AddIdea.fromJson(json as Map<String, dynamic>));
      case 'add_starter_guidelines':
        return CommandAddStarterGuidelines(
          keys: (json['keys'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
        );
      case 'answer_question':
        return CommandAnswerQuestion(value: AnswerQuestion.fromJson(json as Map<String, dynamic>));
      case 'approve_plan':
        return CommandApprovePlan(
          decision: PlanApproval.fromJson(json['decision'] as String),
          feedback: (json['feedback'] == null ? null : (json['feedback'] as String)),
          ticketId: (json['ticket_id'] as String),
        );
      case 'archive_guideline':
        return CommandArchiveGuideline(
          guidelineId: (json['guideline_id'] as String),
        );
      case 'archive_ticket':
        return CommandArchiveTicket(
          ticketId: (json['ticket_id'] as String),
        );
      case 'assign_task':
        return CommandAssignTask(value: AssignTask.fromJson(json as Map<String, dynamic>));
      case 'assign_ticket':
        return CommandAssignTicket(
          employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
          reason: (json['reason'] == null ? null : (json['reason'] as String)),
          ticketId: (json['ticket_id'] as String),
        );
      case 'assign_work_session':
        return CommandAssignWorkSession(value: AssignWorkSession.fromJson(json as Map<String, dynamic>));
      case 'attach_file':
        return CommandAttachFile(value: AttachFile.fromJson(json as Map<String, dynamic>));
      case 'cancel_scheduled_message':
        return CommandCancelScheduledMessage(value: CancelScheduledMessage.fromJson(json as Map<String, dynamic>));
      case 'cancel_task':
        return CommandCancelTask(value: CancelTask.fromJson(json as Map<String, dynamic>));
      case 'cancel_ticket':
        return CommandCancelTicket(
          ticketId: (json['ticket_id'] as String),
        );
      case 'consolidate_memory':
        return CommandConsolidateMemory(value: ConsolidateMemory.fromJson(json as Map<String, dynamic>));
      case 'create_employee':
        return CommandCreateEmployee(value: CreateEmployee.fromJson(json as Map<String, dynamic>));
      case 'create_guideline':
        return CommandCreateGuideline(
          links: (json['links'] == null ? const [] : (json['links'] as List<dynamic>).map((x0) => GuidelineTarget.fromJson(x0 as Map<String, dynamic>)).toList()),
          text: (json['text'] as String),
          title: (json['title'] as String),
        );
      case 'create_project':
        return CommandCreateProject(value: CreateProject.fromJson(json as Map<String, dynamic>));
      case 'create_task':
        return CommandCreateTask(value: CreateTask.fromJson(json as Map<String, dynamic>));
      case 'create_task_schedule':
        return CommandCreateTaskSchedule(value: CreateTaskSchedule.fromJson(json as Map<String, dynamic>));
      case 'create_ticket':
        return CommandCreateTicket(
          confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
          deadlineMs: (json['deadline_ms'] == null ? null : (json['deadline_ms'] as num).toInt()),
          description: (json['description'] == null ? null : (json['description'] as String)),
          files: (json['files'] == null ? const [] : (json['files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
          fromIdea: (json['from_idea'] == null ? null : (json['from_idea'] as String)),
          fromMessage: (json['from_message'] == null ? null : (json['from_message'] as String)),
          kind: (json['kind'] == null ? null : TicketKind.fromJson(json['kind'] as String)),
          labels: (json['labels'] == null ? const [] : (json['labels'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
          ownerEmployeeId: (json['owner_employee_id'] == null ? null : (json['owner_employee_id'] as String)),
          priority: (json['priority'] == null ? null : TicketPriority.fromJson(json['priority'] as String)),
          projectId: (json['project_id'] as String),
          responsibleUserId: (json['responsible_user_id'] == null ? null : (json['responsible_user_id'] as String)),
          run: (json['run'] == null ? null : TaskRun.fromJson(json['run'] as Map<String, dynamic>)),
          start: (json['start'] == null ? null : (json['start'] as bool)),
          title: (json['title'] as String),
        );
      case 'decide_approval':
        return CommandDecideApproval(value: DecideApproval.fromJson(json as Map<String, dynamic>));
      case 'decide_guideline_proposal':
        return CommandDecideGuidelineProposal(
          approve: (json['approve'] as bool),
          links: (json['links'] == null ? null : (json['links'] as List<dynamic>).map((x0) => GuidelineTarget.fromJson(x0 as Map<String, dynamic>)).toList()),
          note: (json['note'] == null ? null : (json['note'] as String)),
          proposalId: (json['proposal_id'] as String),
          text: (json['text'] == null ? null : (json['text'] as String)),
          title: (json['title'] == null ? null : (json['title'] as String)),
        );
      case 'decide_memory':
        return CommandDecideMemory(value: DecideMemory.fromJson(json as Map<String, dynamic>));
      case 'decide_plan':
        return CommandDecidePlan(value: DecidePlan.fromJson(json as Map<String, dynamic>));
      case 'decide_proposal':
        return CommandDecideProposal(value: DecideProposal.fromJson(json as Map<String, dynamic>));
      case 'delete_conversation':
        return CommandDeleteConversation(value: DeleteConversation.fromJson(json as Map<String, dynamic>));
      case 'delete_idea':
        return CommandDeleteIdea(value: DeleteIdea.fromJson(json as Map<String, dynamic>));
      case 'delete_message':
        return CommandDeleteMessage(value: DeleteMessage.fromJson(json as Map<String, dynamic>));
      case 'delete_queued_message':
        return CommandDeleteQueuedMessage(value: DeleteQueuedMessage.fromJson(json as Map<String, dynamic>));
      case 'delete_task_schedule':
        return CommandDeleteTaskSchedule(value: DeleteTaskSchedule.fromJson(json as Map<String, dynamic>));
      case 'dispatch_work_session':
        return CommandDispatchWorkSession(value: DispatchWorkSession.fromJson(json as Map<String, dynamic>));
      case 'edit_message':
        return CommandEditMessage(value: EditMessage.fromJson(json as Map<String, dynamic>));
      case 'follow_ticket':
        return CommandFollowTicket(value: FollowTicket.fromJson(json as Map<String, dynamic>));
      case 'forget_memory':
        return CommandForgetMemory(value: ForgetMemory.fromJson(json as Map<String, dynamic>));
      case 'link_guideline':
        return CommandLinkGuideline(
          guidelineId: (json['guideline_id'] as String),
          target: GuidelineTarget.fromJson(json['target'] as Map<String, dynamic>),
        );
      case 'move_ticket':
        return CommandMoveTicket(
          beforeTicketId: (json['before_ticket_id'] == null ? null : (json['before_ticket_id'] as String)),
          columnId: (json['column_id'] as String),
          ticketId: (json['ticket_id'] as String),
        );
      case 'open_employee_chat':
        return CommandOpenEmployeeChat(value: OpenEmployeeChat.fromJson(json as Map<String, dynamic>));
      case 'pin_memory':
        return CommandPinMemory(value: PinMemory.fromJson(json as Map<String, dynamic>));
      case 'post_ticket_message':
        return CommandPostTicketMessage(
          confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
          files: (json['files'] == null ? const [] : (json['files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
          images: (json['images'] == null ? const [] : (json['images'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
          text: (json['text'] as String),
          threadParent: (json['thread_parent'] == null ? null : (json['thread_parent'] as String)),
          ticketId: (json['ticket_id'] as String),
        );
      case 'react_to_message':
        return CommandReactToMessage(value: ReactToMessage.fromJson(json as Map<String, dynamic>));
      case 'remember':
        return CommandRemember(value: Remember.fromJson(json as Map<String, dynamic>));
      case 'remove_attachment':
        return CommandRemoveAttachment(value: RemoveAttachment.fromJson(json as Map<String, dynamic>));
      case 'remove_connection':
        return CommandRemoveConnection(value: RemoveConnection.fromJson(json as Map<String, dynamic>));
      case 'rename_project':
        return CommandRenameProject(value: RenameProject.fromJson(json as Map<String, dynamic>));
      case 'restore_employee_instructions':
        return CommandRestoreEmployeeInstructions(value: RestoreEmployeeInstructions.fromJson(json as Map<String, dynamic>));
      case 'restore_guideline':
        return CommandRestoreGuideline(
          guidelineId: (json['guideline_id'] as String),
        );
      case 'restore_guideline_version':
        return CommandRestoreGuidelineVersion(
          guidelineId: (json['guideline_id'] as String),
          version: (json['version'] as num).toInt(),
        );
      case 'retry_reply':
        return CommandRetryReply(value: RetryReply.fromJson(json as Map<String, dynamic>));
      case 'retry_session_summary':
        return CommandRetrySessionSummary(value: RetrySessionSummary.fromJson(json as Map<String, dynamic>));
      case 'retry_ticket_summary':
        return CommandRetryTicketSummary(
          ticketId: (json['ticket_id'] as String),
        );
      case 'revise_memory':
        return CommandReviseMemory(value: ReviseMemory.fromJson(json as Map<String, dynamic>));
      case 'revise_plan':
        return CommandRevisePlan(
          confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
          feedback: (json['feedback'] as String),
          ticketId: (json['ticket_id'] as String),
        );
      case 'run_task_schedule_now':
        return CommandRunTaskScheduleNow(value: RunTaskScheduleNow.fromJson(json as Map<String, dynamic>));
      case 'send_back_ticket':
        return CommandSendBackTicket(
          confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
          feedback: (json['feedback'] as String),
          ticketId: (json['ticket_id'] as String),
        );
      case 'send_chat_message':
        return CommandSendChatMessage(value: SendChatMessage.fromJson(json as Map<String, dynamic>));
      case 'send_employee_message':
        return CommandSendEmployeeMessage(value: SendEmployeeMessage.fromJson(json as Map<String, dynamic>));
      case 'send_group_message':
        return CommandSendGroupMessage(value: SendGroupMessage.fromJson(json as Map<String, dynamic>));
      case 'send_project_message':
        return CommandSendProjectMessage(value: SendProjectMessage.fromJson(json as Map<String, dynamic>));
      case 'send_queued_now':
        return CommandSendQueuedNow(value: SendQueuedNow.fromJson(json as Map<String, dynamic>));
      case 'set_agent_mode':
        return CommandSetAgentMode(value: SetAgentMode.fromJson(json as Map<String, dynamic>));
      case 'set_background_login':
        return CommandSetBackgroundLogin(
          enabled: (json['enabled'] as bool),
        );
      case 'set_browser_allowed':
        return CommandSetBrowserAllowed(value: SetBrowserAllowed.fromJson(json as Map<String, dynamic>));
      case 'set_conversation_run':
        return CommandSetConversationRun(value: SetConversationRun.fromJson(json as Map<String, dynamic>));
      case 'set_employee_folder':
        return CommandSetEmployeeFolder(value: SetEmployeeFolder.fromJson(json as Map<String, dynamic>));
      case 'set_employee_reports_to':
        return CommandSetEmployeeReportsTo(value: SetEmployeeReportsTo.fromJson(json as Map<String, dynamic>));
      case 'set_employee_status':
        return CommandSetEmployeeStatus(value: SetEmployeeStatus.fromJson(json as Map<String, dynamic>));
      case 'set_follow_org_chart':
        return CommandSetFollowOrgChart(value: SetFollowOrgChart.fromJson(json as Map<String, dynamic>));
      case 'set_idea_status':
        return CommandSetIdeaStatus(value: SetIdeaStatus.fromJson(json as Map<String, dynamic>));
      case 'set_project_description':
        return CommandSetProjectDescription(value: SetProjectDescription.fromJson(json as Map<String, dynamic>));
      case 'set_project_folder':
        return CommandSetProjectFolder(value: SetProjectFolder.fromJson(json as Map<String, dynamic>));
      case 'set_project_status':
        return CommandSetProjectStatus(value: SetProjectStatus.fromJson(json as Map<String, dynamic>));
      case 'set_run_defaults':
        return CommandSetRunDefaults(value: SetRunDefaults.fromJson(json as Map<String, dynamic>));
      case 'set_screen_allowed':
        return CommandSetScreenAllowed(value: SetScreenAllowed.fromJson(json as Map<String, dynamic>));
      case 'set_server_name':
        return CommandSetServerName(
          name: (json['name'] as String),
        );
      case 'set_task_schedule_paused':
        return CommandSetTaskSchedulePaused(value: SetTaskSchedulePaused.fromJson(json as Map<String, dynamic>));
      case 'set_ticket_responsible':
        return CommandSetTicketResponsible(
          responsibleUserId: (json['responsible_user_id'] == null ? null : (json['responsible_user_id'] as String)),
          ticketId: (json['ticket_id'] as String),
        );
      case 'start_idea':
        return CommandStartIdea(value: StartIdea.fromJson(json as Map<String, dynamic>));
      case 'start_project_conversation':
        return CommandStartProjectConversation(value: StartProjectConversation.fromJson(json as Map<String, dynamic>));
      case 'start_ticket':
        return CommandStartTicket(
          employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
          ticketId: (json['ticket_id'] as String),
        );
      case 'start_work_session':
        return CommandStartWorkSession(value: StartWorkSession.fromJson(json as Map<String, dynamic>));
      case 'stop_run':
        return CommandStopRun(value: StopRun.fromJson(json as Map<String, dynamic>));
      case 'unlink_guideline':
        return CommandUnlinkGuideline(
          guidelineId: (json['guideline_id'] as String),
          target: GuidelineTarget.fromJson(json['target'] as Map<String, dynamic>),
        );
      case 'update_board_columns':
        return CommandUpdateBoardColumns(
          columns: (json['columns'] as List<dynamic>).map((x0) => BoardColumnEdit.fromJson(x0 as Map<String, dynamic>)).toList(),
        );
      case 'update_company':
        return CommandUpdateCompany(value: UpdateCompany.fromJson(json as Map<String, dynamic>));
      case 'update_employee':
        return CommandUpdateEmployee(value: UpdateEmployee.fromJson(json as Map<String, dynamic>));
      case 'update_guideline':
        return CommandUpdateGuideline(
          guidelineId: (json['guideline_id'] as String),
          text: (json['text'] == null ? null : (json['text'] as String)),
          title: (json['title'] == null ? null : (json['title'] as String)),
        );
      case 'update_idea':
        return CommandUpdateIdea(value: UpdateIdea.fromJson(json as Map<String, dynamic>));
      case 'update_project':
        return CommandUpdateProject(value: UpdateProject.fromJson(json as Map<String, dynamic>));
      case 'update_task_schedule':
        return CommandUpdateTaskSchedule(value: UpdateTaskSchedule.fromJson(json as Map<String, dynamic>));
      case 'update_ticket':
        return CommandUpdateTicket(
          addFiles: (json['add_files'] == null ? null : (json['add_files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
          clearDeadline: (json['clear_deadline'] == null ? null : (json['clear_deadline'] as bool)),
          confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
          deadlineMs: (json['deadline_ms'] == null ? null : (json['deadline_ms'] as num).toInt()),
          description: (json['description'] == null ? null : (json['description'] as String)),
          designUrl: (json['design_url'] == null ? null : (json['design_url'] as String)),
          labels: (json['labels'] == null ? null : (json['labels'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
          priority: (json['priority'] == null ? null : TicketPriority.fromJson(json['priority'] as String)),
          run: (json['run'] == null ? null : TaskRun.fromJson(json['run'] as Map<String, dynamic>)),
          ticketId: (json['ticket_id'] as String),
          title: (json['title'] == null ? null : (json['title'] as String)),
        );
      case 'use_my_login':
        return CommandUseMyLogin(
          ticketId: (json['ticket_id'] as String),
        );
      case 'verify_connection':
        return CommandVerifyConnection(value: VerifyConnection.fromJson(json as Map<String, dynamic>));
      default:
        throw FormatException('Unknown Command type: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

/// Review → Done. Only a board member; only when no linked task still works (B6).
final class CommandAcceptTicket extends Command {
  const CommandAcceptTicket({
    this.confirmSecret,
    this.note,
    required this.ticketId,
  });

  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final String? note;
  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'accept_ticket',
      if (confirmSecret != null) 'confirm_secret': confirmSecret,
      if (note != null) 'note': note,
      'ticket_id': ticketId,
    };
}

final class CommandAddConnection extends Command {
  const CommandAddConnection({
    required this.value,
  });

  final AddConnection value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'add_connection',
      ...value.toJson(),
    };
}

final class CommandAddIdea extends Command {
  const CommandAddIdea({
    required this.value,
  });

  final AddIdea value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'add_idea',
      ...value.toJson(),
    };
}

/// `guideline.add_starter_set`: the chosen templates as guidelines without links (B7).
/// A key whose title is in the library already is skipped.
final class CommandAddStarterGuidelines extends Command {
  const CommandAddStarterGuidelines({
    required this.keys,
  });

  final List<String> keys;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'add_starter_guidelines',
      'keys': keys,
    };
}

final class CommandAnswerQuestion extends Command {
  const CommandAnswerQuestion({
    required this.value,
  });

  final AnswerQuestion value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'answer_question',
      ...value.toJson(),
    };
}

/// Plan review → Doing: the owner carries the plan out.
final class CommandApprovePlan extends Command {
  const CommandApprovePlan({
    required this.decision,
    this.feedback,
    required this.ticketId,
  });

  final PlanApproval decision;
  final String? feedback;
  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'approve_plan',
      'decision': decision.toJson(),
      if (feedback != null) 'feedback': feedback,
      'ticket_id': ticketId,
    };
}

/// `guideline.archive`: no run gets it any more; links and versions stay.
final class CommandArchiveGuideline extends Command {
  const CommandArchiveGuideline({
    required this.guidelineId,
  });

  final String guidelineId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'archive_guideline',
      'guideline_id': guidelineId,
    };
}

/// Take a ticket off the board (from Done or Backlog); it stays readable.
final class CommandArchiveTicket extends Command {
  const CommandArchiveTicket({
    required this.ticketId,
  });

  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'archive_ticket',
      'ticket_id': ticketId,
    };
}

final class CommandAssignTask extends Command {
  const CommandAssignTask({
    required this.value,
  });

  final AssignTask value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'assign_task',
      ...value.toJson(),
    };
}

/// Choose the owner of a ticket before its main task exists: a board member
/// (instead of the dispatcher) or the dispatcher itself. Gives `ticket.dispatched`.
///
/// `employee_id` none: no owner again, so the dispatcher chooses when it
/// starts. Only in Backlog (before the start); gives `ticket.updated` with
/// `changed: ["owner"]`, not `ticket.dispatched` (nobody was chosen).
final class CommandAssignTicket extends Command {
  const CommandAssignTicket({
    this.employeeId,
    this.reason,
    required this.ticketId,
  });

  final String? employeeId;
  final String? reason;
  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'assign_ticket',
      if (employeeId != null) 'employee_id': employeeId,
      if (reason != null) 'reason': reason,
      'ticket_id': ticketId,
    };
}

final class CommandAssignWorkSession extends Command {
  const CommandAssignWorkSession({
    required this.value,
  });

  final AssignWorkSession value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'assign_work_session',
      ...value.toJson(),
    };
}

final class CommandAttachFile extends Command {
  const CommandAttachFile({
    required this.value,
  });

  final AttachFile value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'attach_file',
      ...value.toJson(),
    };
}

final class CommandCancelScheduledMessage extends Command {
  const CommandCancelScheduledMessage({
    required this.value,
  });

  final CancelScheduledMessage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'cancel_scheduled_message',
      ...value.toJson(),
    };
}

final class CommandCancelTask extends Command {
  const CommandCancelTask({
    required this.value,
  });

  final CancelTask value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'cancel_task',
      ...value.toJson(),
    };
}

/// Stop the work on a ticket: its main task is cancelled and it goes back to Backlog.
final class CommandCancelTicket extends Command {
  const CommandCancelTicket({
    required this.ticketId,
  });

  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'cancel_ticket',
      'ticket_id': ticketId,
    };
}

final class CommandConsolidateMemory extends Command {
  const CommandConsolidateMemory({
    required this.value,
  });

  final ConsolidateMemory value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'consolidate_memory',
      ...value.toJson(),
    };
}

final class CommandCreateEmployee extends Command {
  const CommandCreateEmployee({
    required this.value,
  });

  final CreateEmployee value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'create_employee',
      ...value.toJson(),
    };
}

/// `guideline.create`: a new guideline (version 1, source `created`), linked at once to `links`.
final class CommandCreateGuideline extends Command {
  const CommandCreateGuideline({
    this.links = const [],
    required this.text,
    required this.title,
  });

  final List<GuidelineTarget> links;
  final String text;
  final String title;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'create_guideline',
      'links': links.map((x0) => x0.toJson()).toList(),
      'text': text,
      'title': title,
    };
}

final class CommandCreateProject extends Command {
  const CommandCreateProject({
    required this.value,
  });

  final CreateProject value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'create_project',
      ...value.toJson(),
    };
}

final class CommandCreateTask extends Command {
  const CommandCreateTask({
    required this.value,
  });

  final CreateTask value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'create_task',
      ...value.toJson(),
    };
}

final class CommandCreateTaskSchedule extends Command {
  const CommandCreateTaskSchedule({
    required this.value,
  });

  final CreateTaskSchedule value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'create_task_schedule',
      ...value.toJson(),
    };
}

/// A new ticket in Backlog (`start`: straight to Todo, as `ticket.start` would).
final class CommandCreateTicket extends Command {
  const CommandCreateTicket({
    this.confirmSecret,
    this.deadlineMs,
    this.description,
    this.files = const [],
    this.fromIdea,
    this.fromMessage,
    this.kind,
    this.labels = const [],
    this.ownerEmployeeId,
    this.priority,
    required this.projectId,
    this.responsibleUserId,
    this.run,
    this.start,
    required this.title,
  });

  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final int? deadlineMs;
  final String? description;
  /// Files to copy into the project's uploads area, as for a task.
  final List<String> files;
  /// Made from this idea (feature `ticket_from_idea`): the ticket gets copies of
  /// the idea's files, and the idea is done and points at the ticket. Optional,
  /// as above: older servers refuse fields they do not know.
  final String? fromIdea;
  /// Made from this chat message ("Make ticket", feature `ticket_discussion`):
  /// the discussion starts with a link to it, and its chat gets a link to the ticket.
  final String? fromMessage;
  /// A quick task (feature `quick_task`): it needs `owner_employee_id`, starts at once
  /// in Doing and ignores `start`. Left out: a normal ticket. Optional, as above:
  /// older servers refuse fields they do not know.
  final TicketKind? kind;
  final List<String> labels;
  /// Chosen now: the dispatcher is skipped when it starts.
  final String? ownerEmployeeId;
  final TicketPriority? priority;
  final String projectId;
  /// The board member responsible from the start (feature `ticket_responsible`).
  /// Left out: nobody. Optional, so a client leaves it out unless it chooses
  /// someone: older servers refuse fields they do not know.
  final String? responsibleUserId;
  /// Mode and model of the main task; unset fields follow the owner.
  final TaskRun? run;
  final bool? start;
  final String title;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'create_ticket',
      if (confirmSecret != null) 'confirm_secret': confirmSecret,
      if (deadlineMs != null) 'deadline_ms': deadlineMs,
      if (description != null) 'description': description,
      'files': files,
      if (fromIdea != null) 'from_idea': fromIdea,
      if (fromMessage != null) 'from_message': fromMessage,
      if (kind != null) 'kind': kind?.toJson(),
      'labels': labels,
      if (ownerEmployeeId != null) 'owner_employee_id': ownerEmployeeId,
      if (priority != null) 'priority': priority?.toJson(),
      'project_id': projectId,
      if (responsibleUserId != null) 'responsible_user_id': responsibleUserId,
      if (run != null) 'run': run?.toJson(),
      if (start != null) 'start': start,
      'title': title,
    };
}

final class CommandDecideApproval extends Command {
  const CommandDecideApproval({
    required this.value,
  });

  final DecideApproval value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'decide_approval',
      ...value.toJson(),
    };
}

/// `guideline.decide_proposal` (B5): approve (with `title`/`text` edited by the
/// board member, and for a new guideline the `links`; none: the suggested
/// ones) or reject. `note` goes back to the proposer. `version_conflict` when
/// the guideline moved on since the proposal's base version.
final class CommandDecideGuidelineProposal extends Command {
  const CommandDecideGuidelineProposal({
    required this.approve,
    this.links,
    this.note,
    required this.proposalId,
    this.text,
    this.title,
  });

  final bool approve;
  final List<GuidelineTarget>? links;
  final String? note;
  final String proposalId;
  final String? text;
  final String? title;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'decide_guideline_proposal',
      'approve': approve,
      if (links != null) 'links': links?.map((x0) => x0.toJson()).toList(),
      if (note != null) 'note': note,
      'proposal_id': proposalId,
      if (text != null) 'text': text,
      if (title != null) 'title': title,
    };
}

final class CommandDecideMemory extends Command {
  const CommandDecideMemory({
    required this.value,
  });

  final DecideMemory value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'decide_memory',
      ...value.toJson(),
    };
}

final class CommandDecidePlan extends Command {
  const CommandDecidePlan({
    required this.value,
  });

  final DecidePlan value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'decide_plan',
      ...value.toJson(),
    };
}

final class CommandDecideProposal extends Command {
  const CommandDecideProposal({
    required this.value,
  });

  final DecideProposal value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'decide_proposal',
      ...value.toJson(),
    };
}

final class CommandDeleteConversation extends Command {
  const CommandDeleteConversation({
    required this.value,
  });

  final DeleteConversation value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'delete_conversation',
      ...value.toJson(),
    };
}

final class CommandDeleteIdea extends Command {
  const CommandDeleteIdea({
    required this.value,
  });

  final DeleteIdea value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'delete_idea',
      ...value.toJson(),
    };
}

final class CommandDeleteMessage extends Command {
  const CommandDeleteMessage({
    required this.value,
  });

  final DeleteMessage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'delete_message',
      ...value.toJson(),
    };
}

final class CommandDeleteQueuedMessage extends Command {
  const CommandDeleteQueuedMessage({
    required this.value,
  });

  final DeleteQueuedMessage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'delete_queued_message',
      ...value.toJson(),
    };
}

final class CommandDeleteTaskSchedule extends Command {
  const CommandDeleteTaskSchedule({
    required this.value,
  });

  final DeleteTaskSchedule value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'delete_task_schedule',
      ...value.toJson(),
    };
}

final class CommandDispatchWorkSession extends Command {
  const CommandDispatchWorkSession({
    required this.value,
  });

  final DispatchWorkSession value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'dispatch_work_session',
      ...value.toJson(),
    };
}

final class CommandEditMessage extends Command {
  const CommandEditMessage({
    required this.value,
  });

  final EditMessage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'edit_message',
      ...value.toJson(),
    };
}

final class CommandFollowTicket extends Command {
  const CommandFollowTicket({
    required this.value,
  });

  final FollowTicket value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'follow_ticket',
      ...value.toJson(),
    };
}

final class CommandForgetMemory extends Command {
  const CommandForgetMemory({
    required this.value,
  });

  final ForgetMemory value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'forget_memory',
      ...value.toJson(),
    };
}

/// `guideline.link`: an employee or a project gets it from its next run.
final class CommandLinkGuideline extends Command {
  const CommandLinkGuideline({
    required this.guidelineId,
    required this.target,
  });

  final String guidelineId;
  final GuidelineTarget target;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'link_guideline',
      'guideline_id': guidelineId,
      'target': target.toJson(),
    };
}

/// Drag a card. Within its column: only the order. To Todo: start it. To
/// Backlog: cancel it. Done only through `ticket.accept`, and Review back to
/// Doing only through `ticket.send_back`; out of Testing only to Backlog (cancel);
/// other moves follow the work and are refused.
/// The place is relative to a neighbour, so it means the same on a board
/// filtered by project as on the whole board. A move within the same column
/// also gives `ticket.moved` (with `from` = `to`). A card put above itself in
/// its own column stays where it is: no event, the version is unchanged.
/// Unknown fields (such as the old `position`) are refused.
final class CommandMoveTicket extends Command {
  const CommandMoveTicket({
    this.beforeTicketId,
    required this.columnId,
    required this.ticketId,
  });

  /// The card it goes right above, in the target column; none: at the bottom.
  final String? beforeTicketId;
  final String columnId;
  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'move_ticket',
      if (beforeTicketId != null) 'before_ticket_id': beforeTicketId,
      'column_id': columnId,
      'ticket_id': ticketId,
    };
}

final class CommandOpenEmployeeChat extends Command {
  const CommandOpenEmployeeChat({
    required this.value,
  });

  final OpenEmployeeChat value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'open_employee_chat',
      ...value.toJson(),
    };
}

final class CommandPinMemory extends Command {
  const CommandPinMemory({
    required this.value,
  });

  final PinMemory value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'pin_memory',
      ...value.toJson(),
    };
}

/// A board member writes in a ticket's discussion (feature `ticket_discussion`).
/// Without `@Name` it goes to the owner while the ticket is in Todo (with an
/// owner), Doing, Plan review, Testing or Review; else it wakes nobody. `@Name` wakes those
/// employees. Refused on Done (the discussion is read-only there). Not for apps.
final class CommandPostTicketMessage extends Command {
  const CommandPostTicketMessage({
    this.confirmSecret,
    this.files = const [],
    this.images = const [],
    required this.text,
    this.threadParent,
    required this.ticketId,
  });

  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  /// Files of any type for the employees (host paths, or this device's upload inbox).
  final List<String> files;
  /// Images that belong to this message (see `SendChatMessage::images`).
  final List<String> images;
  final String text;
  /// A reply in the thread of this message of the discussion (feature
  /// `chat_threads`): a message of the same discussion that is not itself a reply.
  final String? threadParent;
  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'post_ticket_message',
      if (confirmSecret != null) 'confirm_secret': confirmSecret,
      'files': files,
      'images': images,
      'text': text,
      if (threadParent != null) 'thread_parent': threadParent,
      'ticket_id': ticketId,
    };
}

final class CommandReactToMessage extends Command {
  const CommandReactToMessage({
    required this.value,
  });

  final ReactToMessage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'react_to_message',
      ...value.toJson(),
    };
}

final class CommandRemember extends Command {
  const CommandRemember({
    required this.value,
  });

  final Remember value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'remember',
      ...value.toJson(),
    };
}

final class CommandRemoveAttachment extends Command {
  const CommandRemoveAttachment({
    required this.value,
  });

  final RemoveAttachment value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'remove_attachment',
      ...value.toJson(),
    };
}

final class CommandRemoveConnection extends Command {
  const CommandRemoveConnection({
    required this.value,
  });

  final RemoveConnection value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'remove_connection',
      ...value.toJson(),
    };
}

final class CommandRenameProject extends Command {
  const CommandRenameProject({
    required this.value,
  });

  final RenameProject value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'rename_project',
      ...value.toJson(),
    };
}

final class CommandRestoreEmployeeInstructions extends Command {
  const CommandRestoreEmployeeInstructions({
    required this.value,
  });

  final RestoreEmployeeInstructions value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'restore_employee_instructions',
      ...value.toJson(),
    };
}

/// `guideline.restore`: back from the archive, with its links (B4 and the title are checked again).
final class CommandRestoreGuideline extends Command {
  const CommandRestoreGuideline({
    required this.guidelineId,
  });

  final String guidelineId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'restore_guideline',
      'guideline_id': guidelineId,
    };
}

/// `guideline.restore_version`: a copy of `version` as the newest (source `restored`).
final class CommandRestoreGuidelineVersion extends Command {
  const CommandRestoreGuidelineVersion({
    required this.guidelineId,
    required this.version,
  });

  final String guidelineId;
  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'restore_guideline_version',
      'guideline_id': guidelineId,
      'version': version,
    };
}

final class CommandRetryReply extends Command {
  const CommandRetryReply({
    required this.value,
  });

  final RetryReply value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'retry_reply',
      ...value.toJson(),
    };
}

final class CommandRetrySessionSummary extends Command {
  const CommandRetrySessionSummary({
    required this.value,
  });

  final RetrySessionSummary value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'retry_session_summary',
      ...value.toJson(),
    };
}

/// Try the summary of a ticket on Done again after it failed (`SystemLineKind::SummaryFailed`).
/// Only a board member; refused when the summary is already there or still being made.
final class CommandRetryTicketSummary extends Command {
  const CommandRetryTicketSummary({
    required this.ticketId,
  });

  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'retry_ticket_summary',
      'ticket_id': ticketId,
    };
}

final class CommandReviseMemory extends Command {
  const CommandReviseMemory({
    required this.value,
  });

  final ReviseMemory value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'revise_memory',
      ...value.toJson(),
    };
}

/// Plan review → Doing in Plan mode: the owner makes a new plan with the feedback.
final class CommandRevisePlan extends Command {
  const CommandRevisePlan({
    this.confirmSecret,
    required this.feedback,
    required this.ticketId,
  });

  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final String feedback;
  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'revise_plan',
      if (confirmSecret != null) 'confirm_secret': confirmSecret,
      'feedback': feedback,
      'ticket_id': ticketId,
    };
}

final class CommandRunTaskScheduleNow extends Command {
  const CommandRunTaskScheduleNow({
    required this.value,
  });

  final RunTaskScheduleNow value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'run_task_schedule_now',
      ...value.toJson(),
    };
}

/// Review → Doing with feedback (required) for the owner.
final class CommandSendBackTicket extends Command {
  const CommandSendBackTicket({
    this.confirmSecret,
    required this.feedback,
    required this.ticketId,
  });

  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final String feedback;
  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'send_back_ticket',
      if (confirmSecret != null) 'confirm_secret': confirmSecret,
      'feedback': feedback,
      'ticket_id': ticketId,
    };
}

final class CommandSendChatMessage extends Command {
  const CommandSendChatMessage({
    required this.value,
  });

  final SendChatMessage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'send_chat_message',
      ...value.toJson(),
    };
}

final class CommandSendEmployeeMessage extends Command {
  const CommandSendEmployeeMessage({
    required this.value,
  });

  final SendEmployeeMessage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'send_employee_message',
      ...value.toJson(),
    };
}

final class CommandSendGroupMessage extends Command {
  const CommandSendGroupMessage({
    required this.value,
  });

  final SendGroupMessage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'send_group_message',
      ...value.toJson(),
    };
}

final class CommandSendProjectMessage extends Command {
  const CommandSendProjectMessage({
    required this.value,
  });

  final SendProjectMessage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'send_project_message',
      ...value.toJson(),
    };
}

final class CommandSendQueuedNow extends Command {
  const CommandSendQueuedNow({
    required this.value,
  });

  final SendQueuedNow value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'send_queued_now',
      ...value.toJson(),
    };
}

final class CommandSetAgentMode extends Command {
  const CommandSetAgentMode({
    required this.value,
  });

  final SetAgentMode value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_agent_mode',
      ...value.toJson(),
    };
}

/// The company's background jobs (memory consolidation, B6) run on the Claude
/// login of the board member who asks (`enabled`), or on none (`false`, only
/// when it is theirs): without one they do not run. Only the own login can be
/// chosen. Answers `CompanyUpdated`.
final class CommandSetBackgroundLogin extends Command {
  const CommandSetBackgroundLogin({
    required this.enabled,
  });

  final bool enabled;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_background_login',
      'enabled': enabled,
    };
}

final class CommandSetBrowserAllowed extends Command {
  const CommandSetBrowserAllowed({
    required this.value,
  });

  final SetBrowserAllowed value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_browser_allowed',
      ...value.toJson(),
    };
}

final class CommandSetConversationRun extends Command {
  const CommandSetConversationRun({
    required this.value,
  });

  final SetConversationRun value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_conversation_run',
      ...value.toJson(),
    };
}

final class CommandSetEmployeeFolder extends Command {
  const CommandSetEmployeeFolder({
    required this.value,
  });

  final SetEmployeeFolder value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_employee_folder',
      ...value.toJson(),
    };
}

final class CommandSetEmployeeReportsTo extends Command {
  const CommandSetEmployeeReportsTo({
    required this.value,
  });

  final SetEmployeeReportsTo value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_employee_reports_to',
      ...value.toJson(),
    };
}

final class CommandSetEmployeeStatus extends Command {
  const CommandSetEmployeeStatus({
    required this.value,
  });

  final SetEmployeeStatus value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_employee_status',
      ...value.toJson(),
    };
}

final class CommandSetFollowOrgChart extends Command {
  const CommandSetFollowOrgChart({
    required this.value,
  });

  final SetFollowOrgChart value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_follow_org_chart',
      ...value.toJson(),
    };
}

final class CommandSetIdeaStatus extends Command {
  const CommandSetIdeaStatus({
    required this.value,
  });

  final SetIdeaStatus value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_idea_status',
      ...value.toJson(),
    };
}

final class CommandSetProjectDescription extends Command {
  const CommandSetProjectDescription({
    required this.value,
  });

  final SetProjectDescription value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_project_description',
      ...value.toJson(),
    };
}

final class CommandSetProjectFolder extends Command {
  const CommandSetProjectFolder({
    required this.value,
  });

  final SetProjectFolder value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_project_folder',
      ...value.toJson(),
    };
}

final class CommandSetProjectStatus extends Command {
  const CommandSetProjectStatus({
    required this.value,
  });

  final SetProjectStatus value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_project_status',
      ...value.toJson(),
    };
}

final class CommandSetRunDefaults extends Command {
  const CommandSetRunDefaults({
    required this.value,
  });

  final SetRunDefaults value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_run_defaults',
      ...value.toJson(),
    };
}

final class CommandSetScreenAllowed extends Command {
  const CommandSetScreenAllowed({
    required this.value,
  });

  final SetScreenAllowed value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_screen_allowed',
      ...value.toJson(),
    };
}

/// Choose the server's name (`server.set_name`). Empty or only spaces puts the
/// computer's name back. The same name as now changes nothing (no event).
/// Answers `ServerNameSet`.
final class CommandSetServerName extends Command {
  const CommandSetServerName({
    required this.name,
  });

  final String name;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_server_name',
      'name': name,
    };
}

final class CommandSetTaskSchedulePaused extends Command {
  const CommandSetTaskSchedulePaused({
    required this.value,
  });

  final SetTaskSchedulePaused value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_task_schedule_paused',
      ...value.toJson(),
    };
}

/// Choose the board member responsible for a ticket (`ticket.set_responsible`,
/// feature `ticket_responsible`). Its own command rather than a field of
/// `UpdateTicket`, where "nobody" and "unchanged" would need two kinds of none.
final class CommandSetTicketResponsible extends Command {
  const CommandSetTicketResponsible({
    this.responsibleUserId,
    required this.ticketId,
  });

  /// None: nobody is responsible any more.
  final String? responsibleUserId;
  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'set_ticket_responsible',
      if (responsibleUserId != null) 'responsible_user_id': responsibleUserId,
      'ticket_id': ticketId,
    };
}

final class CommandStartIdea extends Command {
  const CommandStartIdea({
    required this.value,
  });

  final StartIdea value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'start_idea',
      ...value.toJson(),
    };
}

final class CommandStartProjectConversation extends Command {
  const CommandStartProjectConversation({
    required this.value,
  });

  final StartProjectConversation value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'start_project_conversation',
      ...value.toJson(),
    };
}

/// The Start button: Backlog to Todo. With an employee the dispatcher is skipped.
/// Also starts again a ticket whose main task failed.
final class CommandStartTicket extends Command {
  const CommandStartTicket({
    this.employeeId,
    required this.ticketId,
  });

  final String? employeeId;
  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'start_ticket',
      if (employeeId != null) 'employee_id': employeeId,
      'ticket_id': ticketId,
    };
}

final class CommandStartWorkSession extends Command {
  const CommandStartWorkSession({
    required this.value,
  });

  final StartWorkSession value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'start_work_session',
      ...value.toJson(),
    };
}

final class CommandStopRun extends Command {
  const CommandStopRun({
    required this.value,
  });

  final StopRun value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'stop_run',
      ...value.toJson(),
    };
}

/// `guideline.unlink`.
final class CommandUnlinkGuideline extends Command {
  const CommandUnlinkGuideline({
    required this.guidelineId,
    required this.target,
  });

  final String guidelineId;
  final GuidelineTarget target;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'unlink_guideline',
      'guideline_id': guidelineId,
      'target': target.toJson(),
    };
}

/// Rename the columns and set their order (B3): all of them, left to right.
/// `expected_version` is the board's (`board_version`: the columns change
/// together); the answer gives the new one.
final class CommandUpdateBoardColumns extends Command {
  const CommandUpdateBoardColumns({
    required this.columns,
  });

  final List<BoardColumnEdit> columns;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'update_board_columns',
      'columns': columns.map((x0) => x0.toJson()).toList(),
    };
}

final class CommandUpdateCompany extends Command {
  const CommandUpdateCompany({
    required this.value,
  });

  final UpdateCompany value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'update_company',
      ...value.toJson(),
    };
}

final class CommandUpdateEmployee extends Command {
  const CommandUpdateEmployee({
    required this.value,
  });

  final UpdateEmployee value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'update_employee',
      ...value.toJson(),
    };
}

/// `guideline.update`: a new version with what changed (source `edited`); what
/// is left out stays. `expected_version` in the envelope is the version the
/// editor started from (`version_conflict` otherwise).
final class CommandUpdateGuideline extends Command {
  const CommandUpdateGuideline({
    required this.guidelineId,
    this.text,
    this.title,
  });

  final String guidelineId;
  final String? text;
  final String? title;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'update_guideline',
      'guideline_id': guidelineId,
      if (text != null) 'text': text,
      if (title != null) 'title': title,
    };
}

final class CommandUpdateIdea extends Command {
  const CommandUpdateIdea({
    required this.value,
  });

  final UpdateIdea value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'update_idea',
      ...value.toJson(),
    };
}

final class CommandUpdateProject extends Command {
  const CommandUpdateProject({
    required this.value,
  });

  final UpdateProject value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'update_project',
      ...value.toJson(),
    };
}

final class CommandUpdateTaskSchedule extends Command {
  const CommandUpdateTaskSchedule({
    required this.value,
  });

  final UpdateTaskSchedule value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'update_task_schedule',
      ...value.toJson(),
    };
}

/// Change the fields of a ticket. Left out: unchanged. The creator never
/// changes; the owner only through `ticket.assign`; mode and model only while
/// the main task does not run.
final class CommandUpdateTicket extends Command {
  const CommandUpdateTicket({
    this.addFiles,
    this.clearDeadline,
    this.confirmSecret,
    this.deadlineMs,
    this.description,
    this.designUrl,
    this.labels,
    this.priority,
    this.run,
    required this.ticketId,
    this.title,
  });

  /// Files to add to the ticket's attachments, as `CreateTicket::files`
  /// (feature `ticket_add_files`): the ones it has stay. A main task that
  /// still works gets them too: once its conversation is open, as an update
  /// there that names them, and a Running one gets a run that reads it (a live
  /// run is not interrupted); before that, through its brief. At most
  /// `ATTACHMENTS_MAX` in all.
  /// Optional rather than a list with a default, so a client leaves it out
  /// unless it adds files: older servers refuse fields they do not know.
  final List<String>? addFiles;
  final bool? clearDeadline;
  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final int? deadlineMs;
  /// An empty text clears it.
  final String? description;
  /// The ticket's design: a Claude Design artifact link (`valid_artifact_url`,
  /// feature `design_links`); an empty text clears it. Left out unless it changes,
  /// so older servers, which refuse unknown fields here, keep working.
  final String? designUrl;
  /// The whole new list.
  final List<String>? labels;
  final TicketPriority? priority;
  final TaskRun? run;
  final String ticketId;
  final String? title;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'update_ticket',
      if (addFiles != null) 'add_files': addFiles,
      if (clearDeadline != null) 'clear_deadline': clearDeadline,
      if (confirmSecret != null) 'confirm_secret': confirmSecret,
      if (deadlineMs != null) 'deadline_ms': deadlineMs,
      if (description != null) 'description': description,
      if (designUrl != null) 'design_url': designUrl,
      if (labels != null) 'labels': labels,
      if (priority != null) 'priority': priority?.toJson(),
      if (run != null) 'run': run?.toJson(),
      'ticket_id': ticketId,
      if (title != null) 'title': title,
    };
}

/// Run a ticket on the Claude login of the board member who asks (B4, D3-3):
/// `TicketView::login_user` becomes that board member, never anyone else. The
/// creator stays. Only a board member; answers `TicketChanged`.
final class CommandUseMyLogin extends Command {
  const CommandUseMyLogin({
    required this.ticketId,
  });

  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'use_my_login',
      'ticket_id': ticketId,
    };
}

final class CommandVerifyConnection extends Command {
  const CommandVerifyConnection({
    required this.value,
  });

  final VerifyConnection value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'verify_connection',
      ...value.toJson(),
    };
}

/// The answer of `command.applied`.
final class CommandApplied {
  const CommandApplied({
    required this.applied,
  });

  /// True: a write with this key committed. False: none did (yet).
  final bool applied;

  factory CommandApplied.fromJson(Map<String, dynamic> json) => CommandApplied(
    applied: (json['applied'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'applied': applied,
  };
}

sealed class CommandResult {
  const CommandResult();

  factory CommandResult.fromJson(Map<String, dynamic> json) {
    final tag = json['type'] as String;
    switch (tag) {
      case 'agent_mode_set':
        return CommandResultAgentModeSet(
          conversationId: (json['conversation_id'] as String),
          mode: AgentMode.fromJson(json['mode'] as String),
        );
      case 'approval_decided':
        return CommandResultApprovalDecided(
          approvalId: (json['approval_id'] as String),
          ok: (json['ok'] as bool),
          output: (json['output'] as String),
          status: ApprovalStatus.fromJson(json['status'] as String),
        );
      case 'attachment_added':
        return CommandResultAttachmentAdded(
          attachmentId: (json['attachment_id'] as String),
          conversationId: (json['conversation_id'] as String),
        );
      case 'attachment_removed':
        return CommandResultAttachmentRemoved(
          attachmentId: (json['attachment_id'] as String),
        );
      case 'board_columns_updated':
        return CommandResultBoardColumnsUpdated(
          version: (json['version'] as num).toInt(),
        );
      case 'browser_allowed_set':
        return CommandResultBrowserAllowedSet(
          allowed: (json['allowed'] as bool),
          conversationId: (json['conversation_id'] as String),
        );
      case 'chat_message_sent':
        return CommandResultChatMessageSent(
          conversationId: (json['conversation_id'] as String),
          messageId: (json['message_id'] as String),
          runId: (json['run_id'] as String),
        );
      case 'company_updated':
        return CommandResultCompanyUpdated(
          version: (json['version'] as num).toInt(),
        );
      case 'connection_added':
        return CommandResultConnectionAdded(
          connectionId: (json['connection_id'] as String),
        );
      case 'connection_removed':
        return CommandResultConnectionRemoved(
          connectionId: (json['connection_id'] as String),
        );
      case 'conversation_deleted':
        return CommandResultConversationDeleted(
          conversationId: (json['conversation_id'] as String),
        );
      case 'conversation_run_set':
        return CommandResultConversationRunSet(
          conversationId: (json['conversation_id'] as String),
        );
      case 'employee_chat_opened':
        return CommandResultEmployeeChatOpened(
          conversationId: (json['conversation_id'] as String),
        );
      case 'employee_created':
        return CommandResultEmployeeCreated(
          conversationId: (json['conversation_id'] as String),
          employeeId: (json['employee_id'] as String),
        );
      case 'employee_folder_set':
        return CommandResultEmployeeFolderSet(
          employeeId: (json['employee_id'] as String),
          folder: (json['folder'] == null ? null : (json['folder'] as String)),
        );
      case 'employee_instructions_restored':
        return CommandResultEmployeeInstructionsRestored(
          employeeId: (json['employee_id'] as String),
          version: (json['version'] as num).toInt(),
        );
      case 'employee_message_sent':
        return CommandResultEmployeeMessageSent(
          conversationId: (json['conversation_id'] as String),
          messageId: (json['message_id'] as String),
          runId: (json['run_id'] as String),
        );
      case 'employee_reports_to_set':
        return CommandResultEmployeeReportsToSet(
          employeeId: (json['employee_id'] as String),
          reportsTo: (json['reports_to'] == null ? null : (json['reports_to'] as String)),
        );
      case 'employee_status_changed':
        return CommandResultEmployeeStatusChanged(
          employeeId: (json['employee_id'] as String),
          status: LifecycleStatus.fromJson(json['status'] as String),
        );
      case 'employee_updated':
        return CommandResultEmployeeUpdated(
          employeeId: (json['employee_id'] as String),
          version: (json['version'] as num).toInt(),
        );
      case 'follow_org_chart_set':
        return CommandResultFollowOrgChartSet(
          follow: (json['follow'] as bool),
        );
      case 'group_message_sent':
        return CommandResultGroupMessageSent(
          conversationId: (json['conversation_id'] as String),
          messageId: (json['message_id'] as String),
          runIds: (json['run_ids'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
          to: (json['to'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
        );
      case 'guideline_changed':
        return CommandResultGuidelineChanged(
          guidelineId: (json['guideline_id'] as String),
          version: (json['version'] as num).toInt(),
        );
      case 'guideline_proposal_decided':
        return CommandResultGuidelineProposalDecided(
          guidelineId: (json['guideline_id'] == null ? null : (json['guideline_id'] as String)),
          proposalId: (json['proposal_id'] as String),
          status: ProposalStatus.fromJson(json['status'] as String),
        );
      case 'idea_added':
        return CommandResultIdeaAdded(
          ideaId: (json['idea_id'] as String),
        );
      case 'idea_deleted':
        return CommandResultIdeaDeleted(
          ideaId: (json['idea_id'] as String),
        );
      case 'idea_started':
        return CommandResultIdeaStarted(
          ideaId: (json['idea_id'] as String),
          taskId: (json['task_id'] as String),
        );
      case 'idea_updated':
        return CommandResultIdeaUpdated(
          ideaId: (json['idea_id'] as String),
        );
      case 'memory_consolidation_queued':
        return CommandResultMemoryConsolidationQueued(
          queued: (json['queued'] as bool),
        );
      case 'memory_decided':
        return CommandResultMemoryDecided(
          memoryId: (json['memory_id'] as String),
          status: MemoryStatus.fromJson(json['status'] as String),
        );
      case 'memory_forgotten':
        return CommandResultMemoryForgotten(
          memoryId: (json['memory_id'] as String),
        );
      case 'memory_stored':
        return CommandResultMemoryStored(
          memoryId: (json['memory_id'] as String),
        );
      case 'message_deleted':
        return CommandResultMessageDeleted(
          conversationId: (json['conversation_id'] as String),
          deletedAtMs: (json['deleted_at_ms'] as num).toInt(),
          messageId: (json['message_id'] as String),
        );
      case 'message_edited':
        return CommandResultMessageEdited(
          conversationId: (json['conversation_id'] as String),
          editedAtMs: (json['edited_at_ms'] as num).toInt(),
          messageId: (json['message_id'] as String),
        );
      case 'message_reacted':
        return CommandResultMessageReacted(
          changed: (json['changed'] as bool),
          conversationId: (json['conversation_id'] as String),
          emoji: (json['emoji'] as String),
          messageId: (json['message_id'] as String),
          on: (json['on'] as bool),
        );
      case 'project_conversation_started':
        return CommandResultProjectConversationStarted(
          conversationId: (json['conversation_id'] as String),
          projectId: (json['project_id'] as String),
        );
      case 'project_created':
        return CommandResultProjectCreated(
          projectId: (json['project_id'] as String),
          version: (json['version'] as num).toInt(),
        );
      case 'project_description_set':
        return CommandResultProjectDescriptionSet(
          projectId: (json['project_id'] as String),
          version: (json['version'] as num).toInt(),
        );
      case 'project_folder_set':
        return CommandResultProjectFolderSet(
          folder: (json['folder'] as String),
          projectId: (json['project_id'] as String),
        );
      case 'project_message_sent':
        return CommandResultProjectMessageSent(
          conversationId: (json['conversation_id'] as String),
          messageId: (json['message_id'] as String),
          projectId: (json['project_id'] as String),
          runId: (json['run_id'] as String),
        );
      case 'project_renamed':
        return CommandResultProjectRenamed(
          projectId: (json['project_id'] as String),
          version: (json['version'] as num).toInt(),
        );
      case 'project_status_changed':
        return CommandResultProjectStatusChanged(
          projectId: (json['project_id'] as String),
          status: LifecycleStatus.fromJson(json['status'] as String),
        );
      case 'project_updated':
        return CommandResultProjectUpdated(
          projectId: (json['project_id'] as String),
          version: (json['version'] as num).toInt(),
        );
      case 'proposal_decided':
        return CommandResultProposalDecided(
          employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
          proposalId: (json['proposal_id'] as String),
          status: ProposalStatus.fromJson(json['status'] as String),
        );
      case 'queued_message_deleted':
        return CommandResultQueuedMessageDeleted(
          conversationId: (json['conversation_id'] as String),
          messageId: (json['message_id'] as String),
          runId: (json['run_id'] as String),
        );
      case 'queued_message_sent_now':
        return CommandResultQueuedMessageSentNow(
          conversationId: (json['conversation_id'] as String),
          messageId: (json['message_id'] as String),
          runId: (json['run_id'] as String),
        );
      case 'reply_retried':
        return CommandResultReplyRetried(
          runId: (json['run_id'] as String),
        );
      case 'run_defaults_set':
        return const CommandResultRunDefaultsSet();
      case 'scheduled_message_cancelled':
        return CommandResultScheduledMessageCancelled(
          messageId: (json['message_id'] as String),
        );
      case 'screen_allowed_set':
        return CommandResultScreenAllowedSet(
          allowed: (json['allowed'] as bool),
          conversationId: (json['conversation_id'] as String),
        );
      case 'server_name_set':
        return CommandResultServerNameSet(
          name: (json['name'] as String),
          version: (json['version'] as num).toInt(),
        );
      case 'session_summary_asked':
        return CommandResultSessionSummaryAsked(
          conversationId: (json['conversation_id'] as String),
        );
      case 'starter_guidelines_added':
        return CommandResultStarterGuidelinesAdded(
          guidelineIds: (json['guideline_ids'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
        );
      case 'stop_requested':
        return CommandResultStopRequested(
          alreadyFinished: (json['already_finished'] as bool),
          runId: (json['run_id'] as String),
        );
      case 'task_assigned':
        return CommandResultTaskAssigned(
          taskId: (json['task_id'] as String),
        );
      case 'task_cancelled':
        return CommandResultTaskCancelled(
          taskId: (json['task_id'] as String),
        );
      case 'task_created':
        return CommandResultTaskCreated(
          taskId: (json['task_id'] as String),
        );
      case 'task_schedule_created':
        return CommandResultTaskScheduleCreated(
          scheduleId: (json['schedule_id'] as String),
        );
      case 'task_schedule_deleted':
        return CommandResultTaskScheduleDeleted(
          scheduleId: (json['schedule_id'] as String),
        );
      case 'task_schedule_ran':
        return CommandResultTaskScheduleRan(
          scheduleId: (json['schedule_id'] as String),
          taskId: (json['task_id'] as String),
        );
      case 'task_schedule_updated':
        return CommandResultTaskScheduleUpdated(
          scheduleId: (json['schedule_id'] as String),
        );
      case 'ticket_changed':
        return CommandResultTicketChanged(
          column: ColumnKind.fromJson(json['column'] as String),
          ticketId: (json['ticket_id'] as String),
          version: (json['version'] as num).toInt(),
        );
      case 'ticket_created':
        return CommandResultTicketCreated(
          number: (json['number'] as num).toInt(),
          ticketId: (json['ticket_id'] as String),
        );
      case 'ticket_follow_changed':
        return CommandResultTicketFollowChanged(
          changed: (json['changed'] as bool),
          on: (json['on'] as bool),
          ticketId: (json['ticket_id'] as String),
        );
      case 'ticket_message_posted':
        return CommandResultTicketMessagePosted(
          conversationId: (json['conversation_id'] as String),
          messageId: (json['message_id'] as String),
          runIds: (json['run_ids'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
          ticketId: (json['ticket_id'] as String),
          to: (json['to'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
        );
      case 'user_created':
        return CommandResultUserCreated(
          userId: (json['user_id'] as String),
        );
      case 'verification_started':
        return CommandResultVerificationStarted(
          connectionId: (json['connection_id'] as String),
        );
      case 'work_session_dispatching':
        return CommandResultWorkSessionDispatching(
          conversationId: (json['conversation_id'] as String),
          messageId: (json['message_id'] as String),
          projectId: (json['project_id'] as String),
        );
      case 'work_session_started':
        return CommandResultWorkSessionStarted(
          conversationId: (json['conversation_id'] as String),
          messageId: (json['message_id'] as String),
          projectId: (json['project_id'] as String),
          runId: (json['run_id'] as String),
        );
      default:
        throw FormatException('Unknown CommandResult type: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

final class CommandResultAgentModeSet extends CommandResult {
  const CommandResultAgentModeSet({
    required this.conversationId,
    required this.mode,
  });

  final String conversationId;
  final AgentMode mode;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'agent_mode_set',
      'conversation_id': conversationId,
      'mode': mode.toJson(),
    };
}

final class CommandResultApprovalDecided extends CommandResult {
  const CommandResultApprovalDecided({
    required this.approvalId,
    required this.ok,
    required this.output,
    required this.status,
  });

  final String approvalId;
  final bool ok;
  final String output;
  final ApprovalStatus status;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'approval_decided',
      'approval_id': approvalId,
      'ok': ok,
      'output': output,
      'status': status.toJson(),
    };
}

final class CommandResultAttachmentAdded extends CommandResult {
  const CommandResultAttachmentAdded({
    required this.attachmentId,
    required this.conversationId,
  });

  final String attachmentId;
  final String conversationId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'attachment_added',
      'attachment_id': attachmentId,
      'conversation_id': conversationId,
    };
}

final class CommandResultAttachmentRemoved extends CommandResult {
  const CommandResultAttachmentRemoved({
    required this.attachmentId,
  });

  final String attachmentId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'attachment_removed',
      'attachment_id': attachmentId,
    };
}

/// The board's new version (`ticket::board_version`).
final class CommandResultBoardColumnsUpdated extends CommandResult {
  const CommandResultBoardColumnsUpdated({
    required this.version,
  });

  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'board_columns_updated',
      'version': version,
    };
}

final class CommandResultBrowserAllowedSet extends CommandResult {
  const CommandResultBrowserAllowedSet({
    required this.allowed,
    required this.conversationId,
  });

  final bool allowed;
  final String conversationId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'browser_allowed_set',
      'allowed': allowed,
      'conversation_id': conversationId,
    };
}

final class CommandResultChatMessageSent extends CommandResult {
  const CommandResultChatMessageSent({
    required this.conversationId,
    required this.messageId,
    required this.runId,
  });

  final String conversationId;
  final String messageId;
  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'chat_message_sent',
      'conversation_id': conversationId,
      'message_id': messageId,
      'run_id': runId,
    };
}

final class CommandResultCompanyUpdated extends CommandResult {
  const CommandResultCompanyUpdated({
    required this.version,
  });

  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'company_updated',
      'version': version,
    };
}

final class CommandResultConnectionAdded extends CommandResult {
  const CommandResultConnectionAdded({
    required this.connectionId,
  });

  final String connectionId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'connection_added',
      'connection_id': connectionId,
    };
}

final class CommandResultConnectionRemoved extends CommandResult {
  const CommandResultConnectionRemoved({
    required this.connectionId,
  });

  final String connectionId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'connection_removed',
      'connection_id': connectionId,
    };
}

final class CommandResultConversationDeleted extends CommandResult {
  const CommandResultConversationDeleted({
    required this.conversationId,
  });

  final String conversationId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'conversation_deleted',
      'conversation_id': conversationId,
    };
}

final class CommandResultConversationRunSet extends CommandResult {
  const CommandResultConversationRunSet({
    required this.conversationId,
  });

  final String conversationId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'conversation_run_set',
      'conversation_id': conversationId,
    };
}

final class CommandResultEmployeeChatOpened extends CommandResult {
  const CommandResultEmployeeChatOpened({
    required this.conversationId,
  });

  final String conversationId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_chat_opened',
      'conversation_id': conversationId,
    };
}

final class CommandResultEmployeeCreated extends CommandResult {
  const CommandResultEmployeeCreated({
    required this.conversationId,
    required this.employeeId,
  });

  final String conversationId;
  final String employeeId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_created',
      'conversation_id': conversationId,
      'employee_id': employeeId,
    };
}

final class CommandResultEmployeeFolderSet extends CommandResult {
  const CommandResultEmployeeFolderSet({
    required this.employeeId,
    this.folder,
  });

  final String employeeId;
  final String? folder;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_folder_set',
      'employee_id': employeeId,
      if (folder != null) 'folder': folder,
    };
}

/// An earlier version of an employee's instructions was put back as `version` (the new current one).
final class CommandResultEmployeeInstructionsRestored extends CommandResult {
  const CommandResultEmployeeInstructionsRestored({
    required this.employeeId,
    required this.version,
  });

  final String employeeId;
  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_instructions_restored',
      'employee_id': employeeId,
      'version': version,
    };
}

final class CommandResultEmployeeMessageSent extends CommandResult {
  const CommandResultEmployeeMessageSent({
    required this.conversationId,
    required this.messageId,
    required this.runId,
  });

  final String conversationId;
  final String messageId;
  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_message_sent',
      'conversation_id': conversationId,
      'message_id': messageId,
      'run_id': runId,
    };
}

final class CommandResultEmployeeReportsToSet extends CommandResult {
  const CommandResultEmployeeReportsToSet({
    required this.employeeId,
    this.reportsTo,
  });

  final String employeeId;
  final String? reportsTo;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_reports_to_set',
      'employee_id': employeeId,
      if (reportsTo != null) 'reports_to': reportsTo,
    };
}

final class CommandResultEmployeeStatusChanged extends CommandResult {
  const CommandResultEmployeeStatusChanged({
    required this.employeeId,
    required this.status,
  });

  final String employeeId;
  final LifecycleStatus status;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_status_changed',
      'employee_id': employeeId,
      'status': status.toJson(),
    };
}

final class CommandResultEmployeeUpdated extends CommandResult {
  const CommandResultEmployeeUpdated({
    required this.employeeId,
    required this.version,
  });

  final String employeeId;
  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_updated',
      'employee_id': employeeId,
      'version': version,
    };
}

final class CommandResultFollowOrgChartSet extends CommandResult {
  const CommandResultFollowOrgChartSet({
    required this.follow,
  });

  final bool follow;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'follow_org_chart_set',
      'follow': follow,
    };
}

/// A message in the group chat, with the employees it woke (each answers in a run of its own).
final class CommandResultGroupMessageSent extends CommandResult {
  const CommandResultGroupMessageSent({
    required this.conversationId,
    required this.messageId,
    required this.runIds,
    required this.to,
  });

  final String conversationId;
  final String messageId;
  final List<String> runIds;
  final List<String> to;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'group_message_sent',
      'conversation_id': conversationId,
      'message_id': messageId,
      'run_ids': runIds,
      'to': to,
    };
}

/// Every guideline write but a proposal's decision and the starter set: the
/// guideline and its current version now.
final class CommandResultGuidelineChanged extends CommandResult {
  const CommandResultGuidelineChanged({
    required this.guidelineId,
    required this.version,
  });

  final String guidelineId;
  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'guideline_changed',
      'guideline_id': guidelineId,
      'version': version,
    };
}

/// `guideline.decide_proposal`; `guideline_id`: the guideline that got the
/// new version (or was made), none for a rejection.
final class CommandResultGuidelineProposalDecided extends CommandResult {
  const CommandResultGuidelineProposalDecided({
    this.guidelineId,
    required this.proposalId,
    required this.status,
  });

  final String? guidelineId;
  final String proposalId;
  final ProposalStatus status;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'guideline_proposal_decided',
      if (guidelineId != null) 'guideline_id': guidelineId,
      'proposal_id': proposalId,
      'status': status.toJson(),
    };
}

final class CommandResultIdeaAdded extends CommandResult {
  const CommandResultIdeaAdded({
    required this.ideaId,
  });

  final String ideaId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'idea_added',
      'idea_id': ideaId,
    };
}

final class CommandResultIdeaDeleted extends CommandResult {
  const CommandResultIdeaDeleted({
    required this.ideaId,
  });

  final String ideaId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'idea_deleted',
      'idea_id': ideaId,
    };
}

final class CommandResultIdeaStarted extends CommandResult {
  const CommandResultIdeaStarted({
    required this.ideaId,
    required this.taskId,
  });

  final String ideaId;
  final String taskId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'idea_started',
      'idea_id': ideaId,
      'task_id': taskId,
    };
}

final class CommandResultIdeaUpdated extends CommandResult {
  const CommandResultIdeaUpdated({
    required this.ideaId,
  });

  final String ideaId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'idea_updated',
      'idea_id': ideaId,
    };
}

/// `queued` is false when a consolidation of that memory is already waiting or running.
final class CommandResultMemoryConsolidationQueued extends CommandResult {
  const CommandResultMemoryConsolidationQueued({
    required this.queued,
  });

  final bool queued;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'memory_consolidation_queued',
      'queued': queued,
    };
}

final class CommandResultMemoryDecided extends CommandResult {
  const CommandResultMemoryDecided({
    required this.memoryId,
    required this.status,
  });

  final String memoryId;
  final MemoryStatus status;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'memory_decided',
      'memory_id': memoryId,
      'status': status.toJson(),
    };
}

final class CommandResultMemoryForgotten extends CommandResult {
  const CommandResultMemoryForgotten({
    required this.memoryId,
  });

  final String memoryId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'memory_forgotten',
      'memory_id': memoryId,
    };
}

final class CommandResultMemoryStored extends CommandResult {
  const CommandResultMemoryStored({
    required this.memoryId,
  });

  final String memoryId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'memory_stored',
      'memory_id': memoryId,
    };
}

/// `message.delete` (feature `message_edits`).
final class CommandResultMessageDeleted extends CommandResult {
  const CommandResultMessageDeleted({
    required this.conversationId,
    required this.deletedAtMs,
    required this.messageId,
  });

  final String conversationId;
  final int deletedAtMs;
  final String messageId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'message_deleted',
      'conversation_id': conversationId,
      'deleted_at_ms': deletedAtMs,
      'message_id': messageId,
    };
}

/// `message.edit` (feature `message_edits`).
final class CommandResultMessageEdited extends CommandResult {
  const CommandResultMessageEdited({
    required this.conversationId,
    required this.editedAtMs,
    required this.messageId,
  });

  final String conversationId;
  final int editedAtMs;
  final String messageId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'message_edited',
      'conversation_id': conversationId,
      'edited_at_ms': editedAtMs,
      'message_id': messageId,
    };
}

/// `message.react` (feature `chat_reactions`): the reaction is as asked now.
/// `changed` is false when it already was (on twice, off when not there).
final class CommandResultMessageReacted extends CommandResult {
  const CommandResultMessageReacted({
    required this.changed,
    required this.conversationId,
    required this.emoji,
    required this.messageId,
    required this.on,
  });

  final bool changed;
  final String conversationId;
  final String emoji;
  final String messageId;
  final bool on;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'message_reacted',
      'changed': changed,
      'conversation_id': conversationId,
      'emoji': emoji,
      'message_id': messageId,
      'on': on,
    };
}

/// A new, empty project conversation.
final class CommandResultProjectConversationStarted extends CommandResult {
  const CommandResultProjectConversationStarted({
    required this.conversationId,
    required this.projectId,
  });

  final String conversationId;
  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'project_conversation_started',
      'conversation_id': conversationId,
      'project_id': projectId,
    };
}

final class CommandResultProjectCreated extends CommandResult {
  const CommandResultProjectCreated({
    required this.projectId,
    required this.version,
  });

  final String projectId;
  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'project_created',
      'project_id': projectId,
      'version': version,
    };
}

final class CommandResultProjectDescriptionSet extends CommandResult {
  const CommandResultProjectDescriptionSet({
    required this.projectId,
    required this.version,
  });

  final String projectId;
  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'project_description_set',
      'project_id': projectId,
      'version': version,
    };
}

final class CommandResultProjectFolderSet extends CommandResult {
  const CommandResultProjectFolderSet({
    required this.folder,
    required this.projectId,
  });

  final String folder;
  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'project_folder_set',
      'folder': folder,
      'project_id': projectId,
    };
}

/// A message in a project conversation was accepted and is being answered.
final class CommandResultProjectMessageSent extends CommandResult {
  const CommandResultProjectMessageSent({
    required this.conversationId,
    required this.messageId,
    required this.projectId,
    required this.runId,
  });

  final String conversationId;
  final String messageId;
  final String projectId;
  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'project_message_sent',
      'conversation_id': conversationId,
      'message_id': messageId,
      'project_id': projectId,
      'run_id': runId,
    };
}

final class CommandResultProjectRenamed extends CommandResult {
  const CommandResultProjectRenamed({
    required this.projectId,
    required this.version,
  });

  final String projectId;
  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'project_renamed',
      'project_id': projectId,
      'version': version,
    };
}

final class CommandResultProjectStatusChanged extends CommandResult {
  const CommandResultProjectStatusChanged({
    required this.projectId,
    required this.status,
  });

  final String projectId;
  final LifecycleStatus status;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'project_status_changed',
      'project_id': projectId,
      'status': status.toJson(),
    };
}

final class CommandResultProjectUpdated extends CommandResult {
  const CommandResultProjectUpdated({
    required this.projectId,
    required this.version,
  });

  final String projectId;
  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'project_updated',
      'project_id': projectId,
      'version': version,
    };
}

/// `employee_id` is the employee that was created when the proposal was approved.
final class CommandResultProposalDecided extends CommandResult {
  const CommandResultProposalDecided({
    this.employeeId,
    required this.proposalId,
    required this.status,
  });

  final String? employeeId;
  final String proposalId;
  final ProposalStatus status;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'proposal_decided',
      if (employeeId != null) 'employee_id': employeeId,
      'proposal_id': proposalId,
      'status': status.toJson(),
    };
}

/// A waiting message was removed and its queued run cancelled.
final class CommandResultQueuedMessageDeleted extends CommandResult {
  const CommandResultQueuedMessageDeleted({
    required this.conversationId,
    required this.messageId,
    required this.runId,
  });

  final String conversationId;
  final String messageId;
  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'queued_message_deleted',
      'conversation_id': conversationId,
      'message_id': messageId,
      'run_id': runId,
    };
}

/// A waiting message's run starts now; the answers ahead of it were stopped.
final class CommandResultQueuedMessageSentNow extends CommandResult {
  const CommandResultQueuedMessageSentNow({
    required this.conversationId,
    required this.messageId,
    required this.runId,
  });

  final String conversationId;
  final String messageId;
  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'queued_message_sent_now',
      'conversation_id': conversationId,
      'message_id': messageId,
      'run_id': runId,
    };
}

final class CommandResultReplyRetried extends CommandResult {
  const CommandResultReplyRetried({
    required this.runId,
  });

  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'reply_retried',
      'run_id': runId,
    };
}

final class CommandResultRunDefaultsSet extends CommandResult {
  const CommandResultRunDefaultsSet();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'run_defaults_set',
    };
}

final class CommandResultScheduledMessageCancelled extends CommandResult {
  const CommandResultScheduledMessageCancelled({
    required this.messageId,
  });

  final String messageId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'scheduled_message_cancelled',
      'message_id': messageId,
    };
}

final class CommandResultScreenAllowedSet extends CommandResult {
  const CommandResultScreenAllowedSet({
    required this.allowed,
    required this.conversationId,
  });

  final bool allowed;
  final String conversationId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'screen_allowed_set',
      'allowed': allowed,
      'conversation_id': conversationId,
    };
}

/// `server.set_name`: the name clients show now and its version (unchanged
/// when the name was the same).
final class CommandResultServerNameSet extends CommandResult {
  const CommandResultServerNameSet({
    required this.name,
    required this.version,
  });

  final String name;
  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'server_name_set',
      'name': name,
      'version': version,
    };
}

/// A work session's summary for the History was asked for (`work_session.retry_summary`).
final class CommandResultSessionSummaryAsked extends CommandResult {
  const CommandResultSessionSummaryAsked({
    required this.conversationId,
  });

  final String conversationId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'session_summary_asked',
      'conversation_id': conversationId,
    };
}

/// `guideline.add_starter_set`: the guidelines made (keys already in the library are skipped).
final class CommandResultStarterGuidelinesAdded extends CommandResult {
  const CommandResultStarterGuidelinesAdded({
    required this.guidelineIds,
  });

  final List<String> guidelineIds;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'starter_guidelines_added',
      'guideline_ids': guidelineIds,
    };
}

final class CommandResultStopRequested extends CommandResult {
  const CommandResultStopRequested({
    required this.alreadyFinished,
    required this.runId,
  });

  final bool alreadyFinished;
  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'stop_requested',
      'already_finished': alreadyFinished,
      'run_id': runId,
    };
}

final class CommandResultTaskAssigned extends CommandResult {
  const CommandResultTaskAssigned({
    required this.taskId,
  });

  final String taskId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'task_assigned',
      'task_id': taskId,
    };
}

final class CommandResultTaskCancelled extends CommandResult {
  const CommandResultTaskCancelled({
    required this.taskId,
  });

  final String taskId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'task_cancelled',
      'task_id': taskId,
    };
}

final class CommandResultTaskCreated extends CommandResult {
  const CommandResultTaskCreated({
    required this.taskId,
  });

  final String taskId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'task_created',
      'task_id': taskId,
    };
}

/// Also the answer to update, pause and resume.
final class CommandResultTaskScheduleCreated extends CommandResult {
  const CommandResultTaskScheduleCreated({
    required this.scheduleId,
  });

  final String scheduleId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'task_schedule_created',
      'schedule_id': scheduleId,
    };
}

final class CommandResultTaskScheduleDeleted extends CommandResult {
  const CommandResultTaskScheduleDeleted({
    required this.scheduleId,
  });

  final String scheduleId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'task_schedule_deleted',
      'schedule_id': scheduleId,
    };
}

final class CommandResultTaskScheduleRan extends CommandResult {
  const CommandResultTaskScheduleRan({
    required this.scheduleId,
    required this.taskId,
  });

  final String scheduleId;
  final String taskId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'task_schedule_ran',
      'schedule_id': scheduleId,
      'task_id': taskId,
    };
}

final class CommandResultTaskScheduleUpdated extends CommandResult {
  const CommandResultTaskScheduleUpdated({
    required this.scheduleId,
  });

  final String scheduleId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'task_schedule_updated',
      'schedule_id': scheduleId,
    };
}

/// The answer to every other ticket write: where the ticket is now.
final class CommandResultTicketChanged extends CommandResult {
  const CommandResultTicketChanged({
    required this.column,
    required this.ticketId,
    required this.version,
  });

  final ColumnKind column;
  final String ticketId;
  final int version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'ticket_changed',
      'column': column.toJson(),
      'ticket_id': ticketId,
      'version': version,
    };
}

final class CommandResultTicketCreated extends CommandResult {
  const CommandResultTicketCreated({
    required this.number,
    required this.ticketId,
  });

  final int number;
  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'ticket_created',
      'number': number,
      'ticket_id': ticketId,
    };
}

/// `ticket.follow` (feature `ticket_follow`): the reader follows it as asked
/// now. `changed` is false when they already did (or did not).
final class CommandResultTicketFollowChanged extends CommandResult {
  const CommandResultTicketFollowChanged({
    required this.changed,
    required this.on,
    required this.ticketId,
  });

  final bool changed;
  final bool on;
  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'ticket_follow_changed',
      'changed': changed,
      'on': on,
      'ticket_id': ticketId,
    };
}

/// A message in a ticket's discussion, with the employees it woke (or the
/// owner it went to: `run_ids` is empty when the main task already runs and takes it).
final class CommandResultTicketMessagePosted extends CommandResult {
  const CommandResultTicketMessagePosted({
    required this.conversationId,
    required this.messageId,
    required this.runIds,
    required this.ticketId,
    required this.to,
  });

  final String conversationId;
  final String messageId;
  final List<String> runIds;
  final String ticketId;
  final List<String> to;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'ticket_message_posted',
      'conversation_id': conversationId,
      'message_id': messageId,
      'run_ids': runIds,
      'ticket_id': ticketId,
      'to': to,
    };
}

/// A new board member (`user.create`).
final class CommandResultUserCreated extends CommandResult {
  const CommandResultUserCreated({
    required this.userId,
  });

  final String userId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'user_created',
      'user_id': userId,
    };
}

final class CommandResultVerificationStarted extends CommandResult {
  const CommandResultVerificationStarted({
    required this.connectionId,
  });

  final String connectionId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'verification_started',
      'connection_id': connectionId,
    };
}

/// A work session was made; the dispatcher chooses who answers its first message.
final class CommandResultWorkSessionDispatching extends CommandResult {
  const CommandResultWorkSessionDispatching({
    required this.conversationId,
    required this.messageId,
    required this.projectId,
  });

  final String conversationId;
  final String messageId;
  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'work_session_dispatching',
      'conversation_id': conversationId,
      'message_id': messageId,
      'project_id': projectId,
    };
}

/// A work session was made and its first message is being answered.
final class CommandResultWorkSessionStarted extends CommandResult {
  const CommandResultWorkSessionStarted({
    required this.conversationId,
    required this.messageId,
    required this.projectId,
    required this.runId,
  });

  final String conversationId;
  final String messageId;
  final String projectId;
  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'work_session_started',
      'conversation_id': conversationId,
      'message_id': messageId,
      'project_id': projectId,
      'run_id': runId,
    };
}

/// `compaction.get_setting` / `compaction.set_threshold` (feature `compaction_setting`):
/// above how many tokens of context a conversation's session is compacted before its
/// next turn. The limits travel along so a client needs no copy of them.
final class CompactionSetting {
  const CompactionSetting({
    required this.defaultTokens,
    required this.maxTokens,
    required this.minTokens,
    required this.thresholdTokens,
  });

  final int defaultTokens;
  final int maxTokens;
  final int minTokens;
  /// The threshold in tokens; 0: never compacted by Aurelius (Claude Code still
  /// compacts by itself when its context window is full).
  final int thresholdTokens;

  factory CompactionSetting.fromJson(Map<String, dynamic> json) => CompactionSetting(
    defaultTokens: (json['default_tokens'] as num).toInt(),
    maxTokens: (json['max_tokens'] as num).toInt(),
    minTokens: (json['min_tokens'] as num).toInt(),
    thresholdTokens: (json['threshold_tokens'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'default_tokens': defaultTokens,
    'max_tokens': maxTokens,
    'min_tokens': minTokens,
    'threshold_tokens': thresholdTokens,
  };
}

/// The company in one read (`company.get`): employees, the org chart setting
/// and its roots (the management team).
final class CompanySnapshot {
  const CompanySnapshot({
    required this.asOfSeq,
    this.backgroundLogin,
    required this.company,
    required this.employees,
    required this.followOrgChart,
    required this.roots,
  });

  final int asOfSeq;
  /// Whose Claude login runs the background jobs (B6); none: they do not run.
  final UserView? backgroundLogin;
  /// The company itself: its name and default dispatcher.
  final CompanyView company;
  /// Employees that are not archived, oldest first.
  final List<EmployeeSummary> employees;
  /// "Follow the org chart" (server setting, on by default).
  final bool followOrgChart;
  /// Active employees without a boss, oldest first: the management team.
  final List<String> roots;

  factory CompanySnapshot.fromJson(Map<String, dynamic> json) => CompanySnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    backgroundLogin: (json['background_login'] == null ? null : UserView.fromJson(json['background_login'] as Map<String, dynamic>)),
    company: CompanyView.fromJson(json['company'] as Map<String, dynamic>),
    employees: (json['employees'] as List<dynamic>).map((x0) => EmployeeSummary.fromJson(x0 as Map<String, dynamic>)).toList(),
    followOrgChart: (json['follow_org_chart'] as bool),
    roots: (json['roots'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    if (backgroundLogin != null) 'background_login': backgroundLogin?.toJson(),
    'company': company.toJson(),
    'employees': employees.map((x0) => x0.toJson()).toList(),
    'follow_org_chart': followOrgChart,
    'roots': roots,
  };
}

final class CompanyView {
  const CompanyView({
    this.dispatcherEmployeeId,
    required this.id,
    required this.name,
    this.testerEmployeeId,
    required this.version,
  });

  /// Chooses the owner of a started ticket without one, unless its project
  /// names its own. None: such tickets wait in Todo for a board member.
  final String? dispatcherEmployeeId;
  final String id;
  final String name;
  /// Runs the full test suite once while a card is in Testing, before Review
  /// (feature `board_testing`). None: no Testing step, cards go from Doing to
  /// Review as before. Left out by older servers.
  final String? testerEmployeeId;
  final int version;

  factory CompanyView.fromJson(Map<String, dynamic> json) => CompanyView(
    dispatcherEmployeeId: (json['dispatcher_employee_id'] == null ? null : (json['dispatcher_employee_id'] as String)),
    id: (json['id'] as String),
    name: (json['name'] as String),
    testerEmployeeId: (json['tester_employee_id'] == null ? null : (json['tester_employee_id'] as String)),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (dispatcherEmployeeId != null) 'dispatcher_employee_id': dispatcherEmployeeId,
    'id': id,
    'name': name,
    if (testerEmployeeId != null) 'tester_employee_id': testerEmployeeId,
    'version': version,
  };
}

sealed class ConfigScope {
  const ConfigScope();

  factory ConfigScope.fromJson(Map<String, dynamic> json) {
    final tag = json['kind'] as String;
    switch (tag) {
      case 'employee':
        return ConfigScopeEmployee(
          id: (json['id'] as String),
        );
      case 'global':
        return const ConfigScopeGlobal();
      case 'project':
        return ConfigScopeProject(
          id: (json['id'] as String),
        );
      default:
        throw FormatException('Unknown ConfigScope kind: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

final class ConfigScopeEmployee extends ConfigScope {
  const ConfigScopeEmployee({
    required this.id,
  });

  final String id;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'employee',
      'id': id,
    };
}

final class ConfigScopeGlobal extends ConfigScope {
  const ConfigScopeGlobal();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'global',
    };
}

final class ConfigScopeProject extends ConfigScope {
  const ConfigScopeProject({
    required this.id,
  });

  final String id;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'project',
      'id': id,
    };
}

final class ConnectionView {
  const ConnectionView({
    required this.authKind,
    required this.billing,
    this.defaultModel,
    this.detail,
    required this.displayName,
    required this.hasSecret,
    required this.id,
    this.lastVerifiedMs,
    required this.models,
    required this.provider,
    this.runtimePath,
    required this.status,
  });

  final String authKind;
  final String billing;
  final String? defaultModel;
  final String? detail;
  final String displayName;
  final bool hasSecret;
  final String id;
  final int? lastVerifiedMs;
  final List<String> models;
  final String provider;
  final String? runtimePath;
  /// disconnected | ready | needs_auth | unavailable
  final String status;

  factory ConnectionView.fromJson(Map<String, dynamic> json) => ConnectionView(
    authKind: (json['auth_kind'] as String),
    billing: (json['billing'] as String),
    defaultModel: (json['default_model'] == null ? null : (json['default_model'] as String)),
    detail: (json['detail'] == null ? null : (json['detail'] as String)),
    displayName: (json['display_name'] as String),
    hasSecret: (json['has_secret'] as bool),
    id: (json['id'] as String),
    lastVerifiedMs: (json['last_verified_ms'] == null ? null : (json['last_verified_ms'] as num).toInt()),
    models: (json['models'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    provider: (json['provider'] as String),
    runtimePath: (json['runtime_path'] == null ? null : (json['runtime_path'] as String)),
    status: (json['status'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'auth_kind': authKind,
    'billing': billing,
    if (defaultModel != null) 'default_model': defaultModel,
    if (detail != null) 'detail': detail,
    'display_name': displayName,
    'has_secret': hasSecret,
    'id': id,
    if (lastVerifiedMs != null) 'last_verified_ms': lastVerifiedMs,
    'models': models,
    'provider': provider,
    if (runtimePath != null) 'runtime_path': runtimePath,
    'status': status,
  };
}

final class ConnectionsSnapshot {
  const ConnectionsSnapshot({
    required this.asOfSeq,
    required this.connections,
    required this.defaults,
  });

  final int asOfSeq;
  final List<ConnectionView> connections;
  final List<DefaultsView> defaults;

  factory ConnectionsSnapshot.fromJson(Map<String, dynamic> json) => ConnectionsSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    connections: (json['connections'] as List<dynamic>).map((x0) => ConnectionView.fromJson(x0 as Map<String, dynamic>)).toList(),
    defaults: (json['defaults'] as List<dynamic>).map((x0) => DefaultsView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'connections': connections.map((x0) => x0.toJson()).toList(),
    'defaults': defaults.map((x0) => x0.toJson()).toList(),
  };
}

/// Merge a scope's memories into short points now, instead of waiting for
/// the background job (which starts by itself once the scope grows).
final class ConsolidateMemory {
  const ConsolidateMemory({
    required this.scope,
  });

  final MemoryScope scope;

  factory ConsolidateMemory.fromJson(Map<String, dynamic> json) => ConsolidateMemory(
    scope: MemoryScope.fromJson(json['scope'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'scope': scope.toJson(),
  };
}

/// One line of a context manifest: what was considered and why it was (not) used.
final class ContextItemView {
  const ContextItemView({
    required this.hash,
    required this.id,
    required this.included,
    required this.label,
    required this.reason,
    this.score,
    required this.section,
    required this.tokens,
  });

  final String hash;
  final String id;
  final bool included;
  final String label;
  final String reason;
  final double? score;
  /// rules | identity | request | turn | summary | memory | source | guideline
  /// (one item per guideline in the run's block, feature `guidelines`; `id` is
  /// `g:<id>@<version>`, never left out by the budget: `included` false only
  /// when B4's safety net dropped it).
  final String section;
  final int tokens;

  factory ContextItemView.fromJson(Map<String, dynamic> json) => ContextItemView(
    hash: (json['hash'] as String),
    id: (json['id'] as String),
    included: (json['included'] as bool),
    label: (json['label'] as String),
    reason: (json['reason'] as String),
    score: (json['score'] == null ? null : (json['score'] as num).toDouble()),
    section: (json['section'] as String),
    tokens: (json['tokens'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'hash': hash,
    'id': id,
    'included': included,
    'label': label,
    'reason': reason,
    if (score != null) 'score': score,
    'section': section,
    'tokens': tokens,
  };
}

final class ContextView {
  const ContextView({
    required this.budgetTokens,
    required this.items,
    required this.runId,
    required this.runtimeManaged,
    required this.scopes,
    required this.usedTokens,
  });

  final int budgetTokens;
  final List<ContextItemView> items;
  final String runId;
  /// The runtime keeps its own history (resume); Aurelius does not see every token.
  final bool runtimeManaged;
  final List<String> scopes;
  final int usedTokens;

  factory ContextView.fromJson(Map<String, dynamic> json) => ContextView(
    budgetTokens: (json['budget_tokens'] as num).toInt(),
    items: (json['items'] as List<dynamic>).map((x0) => ContextItemView.fromJson(x0 as Map<String, dynamic>)).toList(),
    runId: (json['run_id'] as String),
    runtimeManaged: (json['runtime_managed'] as bool),
    scopes: (json['scopes'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    usedTokens: (json['used_tokens'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'budget_tokens': budgetTokens,
    'items': items.map((x0) => x0.toJson()).toList(),
    'run_id': runId,
    'runtime_managed': runtimeManaged,
    'scopes': scopes,
    'used_tokens': usedTokens,
  };
}

final class ConversationSummary {
  const ConversationSummary({
    required this.createdAtMs,
    required this.id,
    required this.messageCount,
    required this.title,
    required this.updatedAtMs,
  });

  final int createdAtMs;
  final String id;
  final int messageCount;
  final String title;
  final int updatedAtMs;

  factory ConversationSummary.fromJson(Map<String, dynamic> json) => ConversationSummary(
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    id: (json['id'] as String),
    messageCount: (json['message_count'] as num).toInt(),
    title: (json['title'] as String),
    updatedAtMs: (json['updated_at_ms'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'created_at_ms': createdAtMs,
    'id': id,
    'message_count': messageCount,
    'title': title,
    'updated_at_ms': updatedAtMs,
  };
}

final class CreateEmployee {
  const CreateEmployee({
    this.accessLevel,
    this.description,
    this.folder,
    this.instructions,
    required this.name,
    this.reportsTo,
    required this.role,
    this.startProfile,
    this.title,
    this.toolPolicy,
  });

  /// What its runs may do with files and commands (C3); none: `Inherit`.
  final AccessLevel? accessLevel;
  /// What other employees read to know when to hand work to this one (P7).
  /// None or empty: their prompt shows the first line of the instructions.
  final String? description;
  /// Optional own workspace folder: the work folder of its chat.
  final String? folder;
  final String? instructions;
  final String name;
  /// Who this employee reports to (not archived); none: a root (the management team).
  final String? reportsTo;
  final String role;
  /// A key of `employee.start_profiles` (C6). What this command leaves out comes from there.
  final String? startProfile;
  /// The position on the org chart ("Head of engineering"); free text, at most 80 characters.
  final String? title;
  /// Which tools and MCP servers it has and which ask first (C2); none: all of its level.
  final ToolPolicy? toolPolicy;

  factory CreateEmployee.fromJson(Map<String, dynamic> json) => CreateEmployee(
    accessLevel: (json['access_level'] == null ? null : AccessLevel.fromJson(json['access_level'] as String)),
    description: (json['description'] == null ? null : (json['description'] as String)),
    folder: (json['folder'] == null ? null : (json['folder'] as String)),
    instructions: (json['instructions'] == null ? null : (json['instructions'] as String)),
    name: (json['name'] as String),
    reportsTo: (json['reports_to'] == null ? null : (json['reports_to'] as String)),
    role: (json['role'] as String),
    startProfile: (json['start_profile'] == null ? null : (json['start_profile'] as String)),
    title: (json['title'] == null ? null : (json['title'] as String)),
    toolPolicy: (json['tool_policy'] == null ? null : ToolPolicy.fromJson(json['tool_policy'] as Map<String, dynamic>)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (accessLevel != null) 'access_level': accessLevel?.toJson(),
    if (description != null) 'description': description,
    if (folder != null) 'folder': folder,
    if (instructions != null) 'instructions': instructions,
    'name': name,
    if (reportsTo != null) 'reports_to': reportsTo,
    'role': role,
    if (startProfile != null) 'start_profile': startProfile,
    if (title != null) 'title': title,
    if (toolPolicy != null) 'tool_policy': toolPolicy?.toJson(),
  };
}

final class CreateProject {
  const CreateProject({
    this.description,
    this.folder,
    required this.name,
  });

  final String? description;
  /// The project's folder; without one Aurelius creates a default folder.
  final String? folder;
  final String name;

  factory CreateProject.fromJson(Map<String, dynamic> json) => CreateProject(
    description: (json['description'] == null ? null : (json['description'] as String)),
    folder: (json['folder'] == null ? null : (json['folder'] as String)),
    name: (json['name'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (description != null) 'description': description,
    if (folder != null) 'folder': folder,
    'name': name,
  };
}

final class CreateTask {
  const CreateTask({
    this.acceptanceCriteria = const [],
    this.confirmSecret,
    this.employeeId,
    this.files = const [],
    required this.goal,
    required this.projectId,
    this.run,
    required this.title,
  });

  final List<String> acceptanceCriteria;
  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  /// Without an employee the task stays a draft.
  final String? employeeId;
  /// Files of any type (images too) to copy into the work folder's
  /// git-ignored uploads area: host paths, or this device's upload inbox
  /// (protocol 1.14; at most 8, 25 MB each).
  final List<String> files;
  final String goal;
  final String projectId;
  /// How the task's conversation runs; unset fields follow the employee.
  final TaskRun? run;
  final String title;

  factory CreateTask.fromJson(Map<String, dynamic> json) => CreateTask(
    acceptanceCriteria: (json['acceptance_criteria'] == null ? const [] : (json['acceptance_criteria'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    files: (json['files'] == null ? const [] : (json['files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    goal: (json['goal'] as String),
    projectId: (json['project_id'] as String),
    run: (json['run'] == null ? null : TaskRun.fromJson(json['run'] as Map<String, dynamic>)),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'acceptance_criteria': acceptanceCriteria,
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    if (employeeId != null) 'employee_id': employeeId,
    'files': files,
    'goal': goal,
    'project_id': projectId,
    if (run != null) 'run': run?.toJson(),
    'title': title,
  };
}

/// A new recurring task for an employee.
final class CreateTaskSchedule {
  const CreateTaskSchedule({
    this.acceptanceCriteria = const [],
    this.confirmSecret,
    required this.employeeId,
    this.files = const [],
    required this.goal,
    this.paused = false,
    required this.projectId,
    required this.recurrence,
    this.run,
    required this.title,
  });

  final List<String> acceptanceCriteria;
  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final String employeeId;
  /// Files to attach to every task made from it (host paths, or this
  /// device's upload inbox); copied when the schedule is saved.
  final List<String> files;
  /// Kept as typed (newlines included), apart from surrounding blank space.
  final String goal;
  /// Start paused: nothing is made until it is resumed.
  final bool paused;
  final String projectId;
  final Recurrence recurrence;
  /// Mode, model and effort of the tasks; unset fields follow the employee.
  final TaskRun? run;
  final String title;

  factory CreateTaskSchedule.fromJson(Map<String, dynamic> json) => CreateTaskSchedule(
    acceptanceCriteria: (json['acceptance_criteria'] == null ? const [] : (json['acceptance_criteria'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    employeeId: (json['employee_id'] as String),
    files: (json['files'] == null ? const [] : (json['files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    goal: (json['goal'] as String),
    paused: (json['paused'] == null ? false : (json['paused'] as bool)),
    projectId: (json['project_id'] as String),
    recurrence: Recurrence.fromJson(json['recurrence'] as Map<String, dynamic>),
    run: (json['run'] == null ? null : TaskRun.fromJson(json['run'] as Map<String, dynamic>)),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'acceptance_criteria': acceptanceCriteria,
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'employee_id': employeeId,
    'files': files,
    'goal': goal,
    'paused': paused,
    'project_id': projectId,
    'recurrence': recurrence.toJson(),
    if (run != null) 'run': run?.toJson(),
    'title': title,
  };
}

/// The user's decision on exactly one action. `action_digest` must match the
/// digest the user was shown; a changed action is a new request.
final class DecideApproval {
  const DecideApproval({
    required this.actionDigest,
    this.alwaysAllow = false,
    required this.approvalId,
    required this.approve,
  });

  final String actionDigest;
  /// With `approve`: also allow this MCP tool from now on, in every run (see
  /// `always_allow_eligible`), and approve its other pending asks.
  final bool alwaysAllow;
  final String approvalId;
  final bool approve;

  factory DecideApproval.fromJson(Map<String, dynamic> json) => DecideApproval(
    actionDigest: (json['action_digest'] as String),
    alwaysAllow: (json['always_allow'] == null ? false : (json['always_allow'] as bool)),
    approvalId: (json['approval_id'] as String),
    approve: (json['approve'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'action_digest': actionDigest,
    'always_allow': alwaysAllow,
    'approval_id': approvalId,
    'approve': approve,
  };
}

/// Accept or reject a proposed memory.
final class DecideMemory {
  const DecideMemory({
    required this.accept,
    required this.memoryId,
  });

  final bool accept;
  final String memoryId;

  factory DecideMemory.fromJson(Map<String, dynamic> json) => DecideMemory(
    accept: (json['accept'] as bool),
    memoryId: (json['memory_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'accept': accept,
    'memory_id': memoryId,
  };
}

/// The user's decision on a plan waiting in the inbox.
final class DecidePlan {
  const DecidePlan({
    required this.approvalId,
    this.confirmSecret,
    required this.decision,
    this.feedback,
  });

  final String approvalId;
  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final PlanDecision decision;
  final String? feedback;

  factory DecidePlan.fromJson(Map<String, dynamic> json) => DecidePlan(
    approvalId: (json['approval_id'] as String),
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    decision: PlanDecision.fromJson(json['decision'] as String),
    feedback: (json['feedback'] == null ? null : (json['feedback'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'approval_id': approvalId,
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'decision': decision.toJson(),
    if (feedback != null) 'feedback': feedback,
  };
}

/// The user's decision. `edits` replaces the proposed fields of a colleague
/// proposal (approve and adjust); `instructions` replaces the proposed text of
/// an instruction proposal. `note` goes back to the proposer, usually with a
/// rejection. Approving an instruction proposal whose base version is no longer
/// the employee's current one is refused (`version_conflict`).
final class DecideProposal {
  const DecideProposal({
    required this.approve,
    this.edits,
    this.instructions,
    this.note,
    required this.proposalId,
  });

  final bool approve;
  final ProposalFields? edits;
  /// Instruction proposals: the text to apply instead of the proposed one.
  final String? instructions;
  final String? note;
  final String proposalId;

  factory DecideProposal.fromJson(Map<String, dynamic> json) => DecideProposal(
    approve: (json['approve'] as bool),
    edits: (json['edits'] == null ? null : ProposalFields.fromJson(json['edits'] as Map<String, dynamic>)),
    instructions: (json['instructions'] == null ? null : (json['instructions'] as String)),
    note: (json['note'] == null ? null : (json['note'] as String)),
    proposalId: (json['proposal_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'approve': approve,
    if (edits != null) 'edits': edits?.toJson(),
    if (instructions != null) 'instructions': instructions,
    if (note != null) 'note': note,
    'proposal_id': proposalId,
  };
}

final class DefaultsView {
  const DefaultsView({
    required this.purpose,
    required this.scope,
    required this.settings,
  });

  final Purpose purpose;
  final ConfigScope scope;
  final RunSettings settings;

  factory DefaultsView.fromJson(Map<String, dynamic> json) => DefaultsView(
    purpose: Purpose.fromJson(json['purpose'] as String),
    scope: ConfigScope.fromJson(json['scope'] as Map<String, dynamic>),
    settings: RunSettings.fromJson(json['settings'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'purpose': purpose.toJson(),
    'scope': scope.toJson(),
    'settings': settings.toJson(),
  };
}

/// Delete a conversation: it disappears and its content (messages, attachments,
/// shared context) is erased. Employee conversations are archived with the employee.
final class DeleteConversation {
  const DeleteConversation({
    required this.conversationId,
  });

  final String conversationId;

  factory DeleteConversation.fromJson(Map<String, dynamic> json) => DeleteConversation(
    conversationId: (json['conversation_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversation_id': conversationId,
  };
}

/// Delete an idea for good, with its attachments (feature `idea_attachments`).
/// A task it became stays, with its own copies of the files.
final class DeleteIdea {
  const DeleteIdea({
    required this.ideaId,
  });

  final String ideaId;

  factory DeleteIdea.fromJson(Map<String, dynamic> json) => DeleteIdea(
    ideaId: (json['idea_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'idea_id': ideaId,
  };
}

/// A board member deletes their own message in a channel or a ticket's discussion
/// (`message.delete`, feature `message_edits`): a line stays that says so. For a
/// message still waiting for its answer see `conversation::DeleteQueuedMessage`.
/// The text stays in the audit log: deleting does not erase a secret.
final class DeleteMessage {
  const DeleteMessage({
    required this.messageId,
  });

  final String messageId;

  factory DeleteMessage.fromJson(Map<String, dynamic> json) => DeleteMessage(
    messageId: (json['message_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'message_id': messageId,
  };
}

/// Delete a user message that still waits for its answer (1.9): its queued
/// run is cancelled and the message is removed. Refused once that run started.
final class DeleteQueuedMessage {
  const DeleteQueuedMessage({
    required this.conversationId,
    required this.messageId,
  });

  final String conversationId;
  final String messageId;

  factory DeleteQueuedMessage.fromJson(Map<String, dynamic> json) => DeleteQueuedMessage(
    conversationId: (json['conversation_id'] as String),
    messageId: (json['message_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversation_id': conversationId,
    'message_id': messageId,
  };
}

final class DeleteTaskSchedule {
  const DeleteTaskSchedule({
    required this.scheduleId,
  });

  final String scheduleId;

  factory DeleteTaskSchedule.fromJson(Map<String, dynamic> json) => DeleteTaskSchedule(
    scheduleId: (json['schedule_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'schedule_id': scheduleId,
  };
}

final class DirEntryView {
  const DirEntryView({
    required this.name,
    required this.path,
  });

  final String name;
  final String path;

  factory DirEntryView.fromJson(Map<String, dynamic> json) => DirEntryView(
    name: (json['name'] as String),
    path: (json['path'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'name': name,
    'path': path,
  };
}

/// `fs.list_dir`: the folders in `path` (never files).
final class DirListing {
  const DirListing({
    required this.entries,
    this.parent,
    this.path,
    required this.roots,
    required this.truncated,
  });

  final List<DirEntryView> entries;
  final String? parent;
  /// The listed folder; `None` when only the starting points are listed.
  final String? path;
  /// Starting points: the projects folder, home, drives.
  final List<DirEntryView> roots;
  /// More folders than were sent.
  final bool truncated;

  factory DirListing.fromJson(Map<String, dynamic> json) => DirListing(
    entries: (json['entries'] as List<dynamic>).map((x0) => DirEntryView.fromJson(x0 as Map<String, dynamic>)).toList(),
    parent: (json['parent'] == null ? null : (json['parent'] as String)),
    path: (json['path'] == null ? null : (json['path'] as String)),
    roots: (json['roots'] as List<dynamic>).map((x0) => DirEntryView.fromJson(x0 as Map<String, dynamic>)).toList(),
    truncated: (json['truncated'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'entries': entries.map((x0) => x0.toJson()).toList(),
    if (parent != null) 'parent': parent,
    if (path != null) 'path': path,
    'roots': roots.map((x0) => x0.toJson()).toList(),
    'truncated': truncated,
  };
}

/// `discussion.audience` (feature `everyone_mention`): whom a board member's
/// `@Everyone` in one discussion would reach, so a client can say so before it
/// sends: it wakes the `coordinator`, the others only know of it. Never the asking
/// board member: they are not told of their own message.
final class DiscussionAudience {
  const DiscussionAudience({
    required this.boardMembers,
    required this.boardUsernames,
    this.coordinator,
    required this.employeeNames,
    required this.employees,
    required this.paused,
    required this.pausedNames,
  });

  /// Board members `@Everyone` tells in the Inbox (and makes follow a ticket).
  final int boardMembers;
  /// Their usernames.
  final List<String> boardUsernames;
  /// The one employee `@Everyone` wakes, who answers or hands over: the ticket's
  /// owner, else the dispatcher, else the CEO, the first that is active. None
  /// when there is nobody, or from a server that still wakes all `employees`.
  final String? coordinator;
  /// Their names, in the order of the company.
  final List<String> employeeNames;
  /// Active employees in the discussion, who know of an `@Everyone`: it is in what
  /// they read when next woken there. It does not wake them (see `coordinator`).
  final int employees;
  /// Employees in the discussion who are paused.
  final int paused;
  /// Their names.
  final List<String> pausedNames;

  factory DiscussionAudience.fromJson(Map<String, dynamic> json) => DiscussionAudience(
    boardMembers: (json['board_members'] as num).toInt(),
    boardUsernames: (json['board_usernames'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    coordinator: (json['coordinator'] == null ? null : (json['coordinator'] as String)),
    employeeNames: (json['employee_names'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    employees: (json['employees'] as num).toInt(),
    paused: (json['paused'] as num).toInt(),
    pausedNames: (json['paused_names'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'board_members': boardMembers,
    'board_usernames': boardUsernames,
    if (coordinator != null) 'coordinator': coordinator,
    'employee_names': employeeNames,
    'employees': employees,
    'paused': paused,
    'paused_names': pausedNames,
  };
}

/// Start a work session and let the dispatcher choose who takes it (feature
/// `session_dispatch`), as it chooses the owner of a ticket: the same as
/// `StartWorkSession` without an employee. Until it has chosen, the session's
/// employee is the dispatcher and nothing more can be written in it.
final class DispatchWorkSession {
  const DispatchWorkSession({
    this.confirmSecret,
    this.files = const [],
    this.images = const [],
    required this.projectId,
    this.run,
    required this.text,
    this.worktree,
  });

  /// See `StartWorkSession::confirm_secret`.
  final bool? confirmSecret;
  /// See `StartWorkSession::files`.
  final List<String> files;
  /// See `StartWorkSession::images`.
  final List<String> images;
  final String projectId;
  /// See `StartWorkSession::run`; it applies to the chosen employee's runs.
  final TaskRun? run;
  final String text;
  /// See `StartWorkSession::worktree`.
  final bool? worktree;

  factory DispatchWorkSession.fromJson(Map<String, dynamic> json) => DispatchWorkSession(
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    files: (json['files'] == null ? const [] : (json['files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    images: (json['images'] == null ? const [] : (json['images'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    projectId: (json['project_id'] as String),
    run: (json['run'] == null ? null : TaskRun.fromJson(json['run'] as Map<String, dynamic>)),
    text: (json['text'] as String),
    worktree: (json['worktree'] == null ? null : (json['worktree'] as bool)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'files': files,
    'images': images,
    'project_id': projectId,
    if (run != null) 'run': run?.toJson(),
    'text': text,
    if (worktree != null) 'worktree': worktree,
  };
}

/// A board member changes the text of their own message in a channel or a
/// ticket's discussion (`message.edit`, feature `message_edits`). It wakes nobody.
/// The earlier text stays in the audit log: editing does not erase a secret.
final class EditMessage {
  const EditMessage({
    this.confirmSecret,
    required this.messageId,
    required this.text,
  });

  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final String messageId;
  final String text;

  factory EditMessage.fromJson(Map<String, dynamic> json) => EditMessage(
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    messageId: (json['message_id'] as String),
    text: (json['text'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'message_id': messageId,
    'text': text,
  };
}

/// One guideline as a run gets it.
final class EffectiveGuideline {
  const EffectiveGuideline({
    required this.chars,
    required this.id,
    required this.level,
    required this.text,
    required this.title,
    required this.version,
  });

  final int chars;
  final String id;
  final GuidelineLevel level;
  final String text;
  final String title;
  final int version;

  factory EffectiveGuideline.fromJson(Map<String, dynamic> json) => EffectiveGuideline(
    chars: (json['chars'] as num).toInt(),
    id: (json['id'] as String),
    level: GuidelineLevel.fromJson(json['level'] as String),
    text: (json['text'] as String),
    title: (json['title'] as String),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'chars': chars,
    'id': id,
    'level': level.toJson(),
    'text': text,
    'title': title,
    'version': version,
  };
}

/// `guideline.effective`: the set a run of this employee in this project would
/// get, in block order, and how it relates to the limit (B4).
final class EffectiveGuidelines {
  const EffectiveGuidelines({
    this.dropped = const [],
    this.employeeId,
    required this.guidelines,
    required this.limitChars,
    this.projectId,
    required this.totalChars,
  });

  /// Left out by the safety net at run start (over the limit; employee ones first).
  final List<String> dropped;
  final String? employeeId;
  final List<EffectiveGuideline> guidelines;
  final int limitChars;
  final String? projectId;
  final int totalChars;

  factory EffectiveGuidelines.fromJson(Map<String, dynamic> json) => EffectiveGuidelines(
    dropped: (json['dropped'] == null ? const [] : (json['dropped'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    guidelines: (json['guidelines'] as List<dynamic>).map((x0) => EffectiveGuideline.fromJson(x0 as Map<String, dynamic>)).toList(),
    limitChars: (json['limit_chars'] as num).toInt(),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
    totalChars: (json['total_chars'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'dropped': dropped,
    if (employeeId != null) 'employee_id': employeeId,
    'guidelines': guidelines.map((x0) => x0.toJson()).toList(),
    'limit_chars': limitChars,
    if (projectId != null) 'project_id': projectId,
    'total_chars': totalChars,
  };
}

/// One of a board member's lasting chats with an employee (feature `employee_chats`).
final class EmployeeChatView {
  const EmployeeChatView({
    required this.conversationId,
    required this.employeeId,
    required this.employeeName,
    required this.lastActivityMs,
    this.preview,
    required this.unread,
  });

  final String conversationId;
  final String employeeId;
  final String employeeName;
  final int lastActivityMs;
  /// The start of the latest message, for a list of chats.
  final String? preview;
  /// What the employee wrote since the board member last read it.
  final int unread;

  factory EmployeeChatView.fromJson(Map<String, dynamic> json) => EmployeeChatView(
    conversationId: (json['conversation_id'] as String),
    employeeId: (json['employee_id'] as String),
    employeeName: (json['employee_name'] as String),
    lastActivityMs: (json['last_activity_ms'] as num).toInt(),
    preview: (json['preview'] == null ? null : (json['preview'] as String)),
    unread: (json['unread'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversation_id': conversationId,
    'employee_id': employeeId,
    'employee_name': employeeName,
    'last_activity_ms': lastActivityMs,
    if (preview != null) 'preview': preview,
    'unread': unread,
  };
}

/// `employee.chats`: the reader's chats that have something in them, the latest first.
final class EmployeeChatsSnapshot {
  const EmployeeChatsSnapshot({
    required this.asOfSeq,
    required this.chats,
  });

  final int asOfSeq;
  final List<EmployeeChatView> chats;

  factory EmployeeChatsSnapshot.fromJson(Map<String, dynamic> json) => EmployeeChatsSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    chats: (json['chats'] as List<dynamic>).map((x0) => EmployeeChatView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'chats': chats.map((x0) => x0.toJson()).toList(),
  };
}

/// One version of an employee's instructions (feature `instruction_proposals`).
final class EmployeeInstructionVersionView {
  const EmployeeInstructionVersionView({
    required this.createdAtMs,
    required this.instructions,
    this.proposalId,
    this.restoredFrom,
    required this.source,
    required this.version,
  });

  final int createdAtMs;
  /// Empty: the employee had no instructions.
  final String instructions;
  /// The approved proposal this version came from.
  final String? proposalId;
  /// The version a restore put back.
  final int? restoredFrom;
  /// `initial` (as it was when versions began), `created`, `edited` (employee.update),
  /// `proposal` (an approved proposal), `restored`.
  final String source;
  final int version;

  factory EmployeeInstructionVersionView.fromJson(Map<String, dynamic> json) => EmployeeInstructionVersionView(
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    instructions: (json['instructions'] as String),
    proposalId: (json['proposal_id'] == null ? null : (json['proposal_id'] as String)),
    restoredFrom: (json['restored_from'] == null ? null : (json['restored_from'] as num).toInt()),
    source: (json['source'] as String),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'created_at_ms': createdAtMs,
    'instructions': instructions,
    if (proposalId != null) 'proposal_id': proposalId,
    if (restoredFrom != null) 'restored_from': restoredFrom,
    'source': source,
    'version': version,
  };
}

final class EmployeeInstructionVersionsSnapshot {
  const EmployeeInstructionVersionsSnapshot({
    required this.currentVersion,
    required this.employeeId,
    required this.versions,
  });

  /// The version in use now.
  final int currentVersion;
  final String employeeId;
  /// Newest first.
  final List<EmployeeInstructionVersionView> versions;

  factory EmployeeInstructionVersionsSnapshot.fromJson(Map<String, dynamic> json) => EmployeeInstructionVersionsSnapshot(
    currentVersion: (json['current_version'] as num).toInt(),
    employeeId: (json['employee_id'] as String),
    versions: (json['versions'] as List<dynamic>).map((x0) => EmployeeInstructionVersionView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'current_version': currentVersion,
    'employee_id': employeeId,
    'versions': versions.map((x0) => x0.toJson()).toList(),
  };
}

/// `employee.queue`: what is in an employee's mailbox, what runs first.
final class EmployeeQueue {
  const EmployeeQueue({
    required this.asOfSeq,
    required this.employeeId,
    required this.items,
  });

  final int asOfSeq;
  final String employeeId;
  final List<EmployeeQueueItem> items;

  factory EmployeeQueue.fromJson(Map<String, dynamic> json) => EmployeeQueue(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    employeeId: (json['employee_id'] as String),
    items: (json['items'] as List<dynamic>).map((x0) => EmployeeQueueItem.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'employee_id': employeeId,
    'items': items.map((x0) => x0.toJson()).toList(),
  };
}

/// One item of an employee's mailbox (U6; feature `employee_queue`).
final class EmployeeQueueItem {
  const EmployeeQueueItem({
    required this.createdAtMs,
    required this.hop,
    required this.id,
    required this.kind,
    required this.notBeforeMs,
    required this.place,
    this.runId,
    required this.source,
    required this.state,
    required this.threadKey,
    this.waitsFor,
  });

  final int createdAtMs;
  final int hop;
  final String id;
  /// message, task, resume, scheduled. Text, not an enum.
  final String kind;
  final int notBeforeMs;
  final ThreadPlace place;
  /// The run that works on it; none while it waits.
  final String? runId;
  /// board, employee or system.
  final String source;
  /// claimed (a run works on it), queued or held.
  final String state;
  final String threadKey;
  /// What holds it back besides its turn: paused, question, limit, later, machine or handoff.
  final String? waitsFor;

  factory EmployeeQueueItem.fromJson(Map<String, dynamic> json) => EmployeeQueueItem(
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    hop: (json['hop'] as num).toInt(),
    id: (json['id'] as String),
    kind: (json['kind'] as String),
    notBeforeMs: (json['not_before_ms'] as num).toInt(),
    place: ThreadPlace.fromJson(json['place'] as Map<String, dynamic>),
    runId: (json['run_id'] == null ? null : (json['run_id'] as String)),
    source: (json['source'] as String),
    state: (json['state'] as String),
    threadKey: (json['thread_key'] as String),
    waitsFor: (json['waits_for'] == null ? null : (json['waits_for'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'created_at_ms': createdAtMs,
    'hop': hop,
    'id': id,
    'kind': kind,
    'not_before_ms': notBeforeMs,
    'place': place.toJson(),
    if (runId != null) 'run_id': runId,
    'source': source,
    'state': state,
    'thread_key': threadKey,
    if (waitsFor != null) 'waits_for': waitsFor,
  };
}

/// What a new run of one employee uses, as the run itself would resolve it.
final class EmployeeRunLine {
  const EmployeeRunLine({
    this.chatEffort,
    this.chatModel,
    this.connection,
    this.effort,
    required this.effortSource,
    required this.employeeId,
    this.model,
    this.modelDefaultEffort,
    required this.modelSource,
    required this.name,
    required this.own,
    required this.status,
    required this.supportedEfforts,
    this.unsupportedEffort,
  });

  /// The effort chosen in the employee's chat; it goes before the rest in its chats.
  final String? chatEffort;
  /// The model chosen in the employee's chat; it goes before the rest in its chats.
  final String? chatModel;
  /// The display name of the connection, if there is one.
  final String? connection;
  /// The effort passed to the run; none: the run gets no effort and the model's own applies.
  final String? effort;
  final RunValueSource effortSource;
  final String employeeId;
  final String? model;
  /// The model's own effort when none is passed, where that is known.
  final String? modelDefaultEffort;
  final RunValueSource modelSource;
  final String name;
  /// What is stored for the employee itself: a change sends it back with only the effort changed.
  final RunSettings own;
  final LifecycleStatus status;
  /// The levels this employee's model can use; empty: it has no effort setting.
  final List<String> supportedEfforts;
  /// A chosen effort the model cannot use, so runs leave it out.
  final String? unsupportedEffort;

  factory EmployeeRunLine.fromJson(Map<String, dynamic> json) => EmployeeRunLine(
    chatEffort: (json['chat_effort'] == null ? null : (json['chat_effort'] as String)),
    chatModel: (json['chat_model'] == null ? null : (json['chat_model'] as String)),
    connection: (json['connection'] == null ? null : (json['connection'] as String)),
    effort: (json['effort'] == null ? null : (json['effort'] as String)),
    effortSource: RunValueSource.fromJson(json['effort_source'] as String),
    employeeId: (json['employee_id'] as String),
    model: (json['model'] == null ? null : (json['model'] as String)),
    modelDefaultEffort: (json['model_default_effort'] == null ? null : (json['model_default_effort'] as String)),
    modelSource: RunValueSource.fromJson(json['model_source'] as String),
    name: (json['name'] as String),
    own: RunSettings.fromJson(json['own'] as Map<String, dynamic>),
    status: LifecycleStatus.fromJson(json['status'] as String),
    supportedEfforts: (json['supported_efforts'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    unsupportedEffort: (json['unsupported_effort'] == null ? null : (json['unsupported_effort'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (chatEffort != null) 'chat_effort': chatEffort,
    if (chatModel != null) 'chat_model': chatModel,
    if (connection != null) 'connection': connection,
    if (effort != null) 'effort': effort,
    'effort_source': effortSource.toJson(),
    'employee_id': employeeId,
    if (model != null) 'model': model,
    if (modelDefaultEffort != null) 'model_default_effort': modelDefaultEffort,
    'model_source': modelSource.toJson(),
    'name': name,
    'own': own.toJson(),
    'status': status.toJson(),
    'supported_efforts': supportedEfforts,
    if (unsupportedEffort != null) 'unsupported_effort': unsupportedEffort,
  };
}

/// `employee.run_overview` (feature `employee_run_overview`).
final class EmployeeRunsView {
  const EmployeeRunsView({
    required this.company,
    this.companyModel,
    this.companyModelDefaultEffort,
    required this.companySupportedEfforts,
    required this.employees,
  });

  /// The stored company-wide default for employee runs.
  final RunSettings company;
  final String? companyModel;
  final String? companyModelDefaultEffort;
  /// The levels the company's default model can use.
  final List<String> companySupportedEfforts;
  final List<EmployeeRunLine> employees;

  factory EmployeeRunsView.fromJson(Map<String, dynamic> json) => EmployeeRunsView(
    company: RunSettings.fromJson(json['company'] as Map<String, dynamic>),
    companyModel: (json['company_model'] == null ? null : (json['company_model'] as String)),
    companyModelDefaultEffort: (json['company_model_default_effort'] == null ? null : (json['company_model_default_effort'] as String)),
    companySupportedEfforts: (json['company_supported_efforts'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    employees: (json['employees'] as List<dynamic>).map((x0) => EmployeeRunLine.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'company': company.toJson(),
    if (companyModel != null) 'company_model': companyModel,
    if (companyModelDefaultEffort != null) 'company_model_default_effort': companyModelDefaultEffort,
    'company_supported_efforts': companySupportedEfforts,
    'employees': employees.map((x0) => x0.toJson()).toList(),
  };
}

final class EmployeeSummary {
  const EmployeeSummary({
    this.accessLevel,
    this.activityState,
    required this.agentMode,
    required this.agents,
    required this.browserAllowed,
    required this.conversationId,
    required this.conversations,
    this.description,
    this.folder,
    required this.id,
    this.instructions,
    required this.name,
    required this.projectsActive,
    this.reportsTo,
    required this.role,
    required this.screenAllowed,
    required this.sequentialTasks,
    this.settings,
    required this.status,
    this.title,
    this.toolPolicy,
    required this.version,
    required this.working,
  });

  /// What its runs may do with files and commands (C3); left out when `Inherit`.
  final AccessLevel? accessLevel;
  /// What it is doing now (U6, derived, never stored); left out when idle.
  final ActivityState? activityState;
  final AgentMode agentMode;
  /// What runs for this employee right now: its own steps,
  /// subagents and sub-tasks delegated to colleagues (derived, never stored).
  final List<AgentView> agents;
  /// The agent may use the user's browser (shares their logins); off by default.
  final bool browserAllowed;
  /// Its persistent chat with the user (company scope, no project; `employee.chat`).
  final String conversationId;
  /// Its chats, most recent first: the persistent one (`conversation_id`, it
  /// carries mode and memory summaries) and older ones from before
  /// protocol 3 (history). Task conversations are not listed.
  final List<ConversationSummary> conversations;
  /// What other employees read about it (P7). Left out when it has none: their
  /// prompt then shows the first line of its instructions.
  final String? description;
  /// Own workspace folder, if any: the work folder of its chat (in a task the project folder applies).
  final String? folder;
  final String id;
  final String? instructions;
  final String name;
  /// Projects it has open tasks in (derived, never stored).
  final List<String> projectsActive;
  /// Who this employee reports to; none: a root of the org chart (the management team).
  final String? reportsTo;
  final String role;
  /// The agent may take screenshots without asking each time; off by default (it then asks).
  final bool screenAllowed;
  /// It works on one task at a time; the others wait their turn.
  final bool sequentialTasks;
  /// Its own provider, model and effort (C1); left out when it inherits them all.
  /// The same as `connection.list` has for its scope, without needing that read.
  final RunSettings? settings;
  final LifecycleStatus status;
  final String? title;
  /// Its tools, MCP servers and what asks first (C2); left out when it has all of its level.
  final ToolPolicy? toolPolicy;
  final int version;
  /// Derived presence: a run of this employee is active (never stored).
  final bool working;

  factory EmployeeSummary.fromJson(Map<String, dynamic> json) => EmployeeSummary(
    accessLevel: (json['access_level'] == null ? null : AccessLevel.fromJson(json['access_level'] as String)),
    activityState: (json['activity_state'] == null ? null : ActivityState.fromJson(json['activity_state'] as String)),
    agentMode: AgentMode.fromJson(json['agent_mode'] as String),
    agents: (json['agents'] as List<dynamic>).map((x0) => AgentView.fromJson(x0 as Map<String, dynamic>)).toList(),
    browserAllowed: (json['browser_allowed'] as bool),
    conversationId: (json['conversation_id'] as String),
    conversations: (json['conversations'] as List<dynamic>).map((x0) => ConversationSummary.fromJson(x0 as Map<String, dynamic>)).toList(),
    description: (json['description'] == null ? null : (json['description'] as String)),
    folder: (json['folder'] == null ? null : (json['folder'] as String)),
    id: (json['id'] as String),
    instructions: (json['instructions'] == null ? null : (json['instructions'] as String)),
    name: (json['name'] as String),
    projectsActive: (json['projects_active'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    reportsTo: (json['reports_to'] == null ? null : (json['reports_to'] as String)),
    role: (json['role'] as String),
    screenAllowed: (json['screen_allowed'] as bool),
    sequentialTasks: (json['sequential_tasks'] as bool),
    settings: (json['settings'] == null ? null : RunSettings.fromJson(json['settings'] as Map<String, dynamic>)),
    status: LifecycleStatus.fromJson(json['status'] as String),
    title: (json['title'] == null ? null : (json['title'] as String)),
    toolPolicy: (json['tool_policy'] == null ? null : ToolPolicy.fromJson(json['tool_policy'] as Map<String, dynamic>)),
    version: (json['version'] as num).toInt(),
    working: (json['working'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (accessLevel != null) 'access_level': accessLevel?.toJson(),
    if (activityState != null) 'activity_state': activityState?.toJson(),
    'agent_mode': agentMode.toJson(),
    'agents': agents.map((x0) => x0.toJson()).toList(),
    'browser_allowed': browserAllowed,
    'conversation_id': conversationId,
    'conversations': conversations.map((x0) => x0.toJson()).toList(),
    if (description != null) 'description': description,
    if (folder != null) 'folder': folder,
    'id': id,
    if (instructions != null) 'instructions': instructions,
    'name': name,
    'projects_active': projectsActive,
    if (reportsTo != null) 'reports_to': reportsTo,
    'role': role,
    'screen_allowed': screenAllowed,
    'sequential_tasks': sequentialTasks,
    if (settings != null) 'settings': settings?.toJson(),
    'status': status.toJson(),
    if (title != null) 'title': title,
    if (toolPolicy != null) 'tool_policy': toolPolicy?.toJson(),
    'version': version,
    'working': working,
  };
}

final class EmployeesSnapshot {
  const EmployeesSnapshot({
    required this.asOfSeq,
    required this.employees,
  });

  final int asOfSeq;
  final List<EmployeeSummary> employees;

  factory EmployeesSnapshot.fromJson(Map<String, dynamic> json) => EmployeesSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    employees: (json['employees'] as List<dynamic>).map((x0) => EmployeeSummary.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'employees': employees.map((x0) => x0.toJson()).toList(),
  };
}

enum ErrorCode {
  /// A ticket, board or company write needs an active board member on the
  /// connection (a session: `user.login`, the first `user.create`, or
  /// `HelloParams::session_token`). The second lock behind `unauthenticated`.
  boardMemberRequired('board_member_required'),
  /// A write in a channel that can only be read (feature `ticket_channels`):
  /// its ticket is on Done or archived, or its project is archived. Only a
  /// connection whose `HelloParams::accepts` names `ticket_channels` gets this
  /// code; any other gets `validation` with the same message, since an older
  /// client cannot read a code it does not know.
  channelArchived('channel_archived'),
  /// The caller lacks the right for this method.
  forbidden('forbidden'),
  frameTooLarge('frame_too_large'),
  /// `guideline.create` / `update` / `restore` / an approval: another guideline
  /// that is not archived has this title (case does not count).
  guidelineTitleTaken('guideline_title_taken'),
  /// A guideline write (save, restore, link, approve) would give a run more
  /// than 16 000 characters of guidelines (B4): `AppError::too_large` names the
  /// combination of employee and project. Nothing was saved.
  guidelineTooLarge('guideline_too_large'),
  idempotencyConflict('idempotency_conflict'),
  incompatibleProtocol('incompatible_protocol'),
  internal('internal'),
  invalidRequest('invalid_request'),
  /// `user.create`: 3 to 32 letters, digits, `.`, `_` or `-`.
  invalidUsername('invalid_username'),
  /// `user.disable`: the last active board member cannot be disabled.
  lastBoardMember('last_board_member'),
  /// A board member's message (an employee's chat, `ticket.post`, the Team
  /// chat) holds the value of a vault secret (B10): nothing was stored.
  /// `AppError::matched_items` names the items (never a value); send again
  /// with `confirm_secret` to store it with the value redacted.
  looksLikeSecret('looks_like_secret'),
  notFound('not_found'),
  /// The session's board member signed in with a temporary password (set by
  /// another board member): only `user.set_password` (own), `user.logout`,
  /// `user.whoami` and `session.hello` work until it is changed.
  passwordChangeRequired('password_change_required'),
  shuttingDown('shutting_down'),
  storageUnavailable('storage_unavailable'),
  /// `user.login` / `user.create`: too many tries; `AppError::retry_after_ms`
  /// says when the next one is taken ("Too many attempts. Try again in 0:30.").
  tooManyAttempts('too_many_attempts'),
  /// Protocol 5: this connection has no (valid) session. Everything but
  /// `session.hello`, `user.whoami`, `user.login`, `user.list` and the first
  /// `user.create` answers this; the client shows its sign-in screen.
  unauthenticated('unauthenticated'),
  /// The server does not know this method (an older server); the connection stays open.
  unknownMethod('unknown_method'),
  /// `user.create`: another board member has this name (case does not count).
  usernameTaken('username_taken'),
  validation('validation'),
  /// `vault.update`: a secret stays a secret and nothing becomes one (B7);
  /// make a new item and archive the old one. Text and url may trade places.
  vaultKindLocked('vault_kind_locked'),
  /// `vault.reveal` / `vault.key_export`: the server's vault key is missing or
  /// does not fit this value (a restore without the key, MU4-04 AC3).
  vaultUndecryptable('vault_undecryptable'),
  versionConflict('version_conflict'),
  /// `user.create` / `user.set_password`: the password is shorter than 12 characters.
  weakPassword('weak_password'),
  /// `user.login`: the username or the password is wrong (one answer for both,
  /// also for a disabled board member); `user.set_password`: the old password is wrong.
  wrongPassword('wrong_password');

  const ErrorCode(this.wire);

  final String wire;

  static ErrorCode fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown ErrorCode value: $value'),
      );

  String toJson() => wire;
}

final class Event {
  const Event({
    required this.aggregateId,
    required this.atMs,
    required this.kind,
    this.projectId,
    required this.seq,
  });

  /// The task, employee, conversation, … the event is about.
  final String aggregateId;
  final int atMs;
  final EventKind kind;
  final String? projectId;
  /// Position in the log; replay with `event.subscribe { after_seq }`.
  final int seq;

  factory Event.fromJson(Map<String, dynamic> json) => Event(
    aggregateId: (json['aggregate_id'] as String),
    atMs: (json['at_ms'] as num).toInt(),
    kind: EventKind.fromJson(json['kind'] as String),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
    seq: (json['seq'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'aggregate_id': aggregateId,
    'at_ms': atMs,
    'kind': kind.toJson(),
    if (projectId != null) 'project_id': projectId,
    'seq': seq,
  };
}

enum EventKind {
  approvalAllowedAlways('approval.allowed_always'),
  approvalAllowedByMode('approval.allowed_by_mode'),
  approvalAnswered('approval.answered'),
  approvalApproved('approval.approved'),
  approvalConsumed('approval.consumed'),
  approvalExpired('approval.expired'),
  approvalInvalidated('approval.invalidated'),
  approvalPlanApproved('approval.plan_approved'),
  approvalPlanRejected('approval.plan_rejected'),
  approvalPlanRevisionRequested('approval.plan_revision_requested'),
  approvalRejected('approval.rejected'),
  approvalRequested('approval.requested'),
  attachmentAdded('attachment.added'),
  attachmentRemoved('attachment.removed'),
  boardColumnsUpdated('board.columns_updated'),
  claudeLoginUpdated('claude_login.updated'),
  companyConverted('company.converted'),
  companyUpdated('company.updated'),
  configChanged('config.changed'),
  connectionChanged('connection.changed'),
  conversationCreated('conversation.created'),
  conversationDeleted('conversation.deleted'),
  conversationSettingsChanged('conversation.settings_changed'),
  conversationStarted('conversation.started'),
  employeeArchived('employee.archived'),
  employeeConfigChanged('employee.config_changed'),
  employeeCreated('employee.created'),
  employeeFolderSet('employee.folder_set'),
  employeeInstructionsProposed('employee.instructions_proposed'),
  employeePaused('employee.paused'),
  employeeProposalDecided('employee.proposal_decided'),
  employeeProposed('employee.proposed'),
  employeeReportsToSet('employee.reports_to_set'),
  employeeResumed('employee.resumed'),
  folderChanged('folder.changed'),
  guidelineArchived('guideline.archived'),
  guidelineCreated('guideline.created'),
  guidelineLinked('guideline.linked'),
  guidelineProposalDecided('guideline.proposal_decided'),
  guidelineProposed('guideline.proposed'),
  guidelineRestored('guideline.restored'),
  guidelineUnlinked('guideline.unlinked'),
  guidelineUpdated('guideline.updated'),
  hostWrite('host.write'),
  ideaAdded('idea.added'),
  ideaDeleted('idea.deleted'),
  ideaStarted('idea.started'),
  ideaStatusChanged('idea.status_changed'),
  ideaTicketCreated('idea.ticket_created'),
  ideaUpdated('idea.updated'),
  mcpAuthChanged('mcp.auth_changed'),
  mcpLoginFinished('mcp.login_finished'),
  memoryAccepted('memory.accepted'),
  memoryConsolidated('memory.consolidated'),
  memoryConsolidationRequested('memory.consolidation_requested'),
  memoryExtracted('memory.extracted'),
  memoryForgotten('memory.forgotten'),
  memoryPinned('memory.pinned'),
  memoryProposed('memory.proposed'),
  memoryReinforced('memory.reinforced'),
  memoryRejected('memory.rejected'),
  memoryRemembered('memory.remembered'),
  memoryRevised('memory.revised'),
  memorySummarized('memory.summarized'),
  memorySuperseded('memory.superseded'),
  messageAssistantCommitted('message.assistant_committed'),
  messageDeleted('message.deleted'),
  messageEdited('message.edited'),
  messageInterrupted('message.interrupted'),
  messageReacted('message.reacted'),
  messageSentNow('message.sent_now'),
  messageUserCommitted('message.user_committed'),
  projectArchived('project.archived'),
  projectCreated('project.created'),
  projectFolderSet('project.folder_set'),
  projectFollowOrgChart('project.follow_org_chart'),
  projectPaused('project.paused'),
  projectUpdated('project.updated'),
  recurringTaskChanged('recurring_task.changed'),
  runCancelRequested('run.cancel_requested'),
  runCancelled('run.cancelled'),
  runCheckpointed('run.checkpointed'),
  runCompleted('run.completed'),
  runFailed('run.failed'),
  runQueued('run.queued'),
  runSourceFetched('run.source_fetched'),
  runStarted('run.started'),
  runUsageReported('run.usage_reported'),
  runWaiting('run.waiting'),
  serverRenamed('server.renamed'),
  settingsFollowOrgChart('settings.follow_org_chart'),
  taskAssigned('task.assigned'),
  taskCancelled('task.cancelled'),
  taskCreated('task.created'),
  taskDelegated('task.delegated'),
  taskFailed('task.failed'),
  taskHandedOff('task.handed_off'),
  taskJobRequeued('task.job_requeued'),
  taskModeChosen('task.mode_chosen'),
  taskPosted('task.posted'),
  taskQueued('task.queued'),
  taskReportReady('task.report_ready'),
  taskResumed('task.resumed'),
  taskRunChanged('task.run_changed'),
  taskStarted('task.started'),
  taskSucceeded('task.succeeded'),
  taskWaiting('task.waiting'),
  ticketAccepted('ticket.accepted'),
  ticketArchived('ticket.archived'),
  ticketCancelled('ticket.cancelled'),
  ticketCreated('ticket.created'),
  ticketDispatched('ticket.dispatched'),
  ticketFollowChanged('ticket.follow_changed'),
  ticketMoved('ticket.moved'),
  ticketPlanApproved('ticket.plan_approved'),
  ticketPlanReview('ticket.plan_review'),
  ticketPlanRevised('ticket.plan_revised'),
  ticketReview('ticket.review'),
  ticketSentBack('ticket.sent_back'),
  ticketStarted('ticket.started'),
  ticketTested('ticket.tested'),
  ticketUpdated('ticket.updated'),
  userCreated('user.created'),
  userDisabled('user.disabled'),
  vaultItemArchived('vault.item_archived'),
  vaultItemCreated('vault.item_created'),
  vaultItemUpdated('vault.item_updated'),
  wakeRequested('wake.requested');

  const EventKind(this.wire);

  final String wire;

  static EventKind fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown EventKind value: $value'),
      );

  String toJson() => wire;
}

/// A file kept with a message: what an agent made or mentioned (a screenshot,
/// a report, a log). Images show in the app; other files open on the device.
final class FileRef {
  const FileRef({
    required this.id,
    required this.mediaType,
    required this.name,
    required this.size,
  });

  final String id;
  final String mediaType;
  final String name;
  final int size;

  factory FileRef.fromJson(Map<String, dynamic> json) => FileRef(
    id: (json['id'] as String),
    mediaType: (json['media_type'] as String),
    name: (json['name'] as String),
    size: (json['size'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'id': id,
    'media_type': mediaType,
    'name': name,
    'size': size,
  };
}

/// Whose work folder a file query looks in.
sealed class FileScope {
  const FileScope();

  factory FileScope.fromJson(Map<String, dynamic> json) {
    final tag = json['kind'] as String;
    switch (tag) {
      case 'conversation':
        return FileScopeConversation(
          conversationId: (json['conversation_id'] as String),
        );
      case 'project':
        return FileScopeProject(
          projectId: (json['project_id'] as String),
        );
      case 'task':
        return FileScopeTask(
          taskId: (json['task_id'] as String),
        );
      default:
        throw FormatException('Unknown FileScope kind: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

/// Where a conversation works: a task's (its workspace, else the project
/// folder), a project conversation's (the project folder), an employee
/// chat's (the employee's own folder).
final class FileScopeConversation extends FileScope {
  const FileScopeConversation({
    required this.conversationId,
  });

  final String conversationId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'conversation',
      'conversation_id': conversationId,
    };
}

/// The project's folder.
final class FileScopeProject extends FileScope {
  const FileScopeProject({
    required this.projectId,
  });

  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'project',
      'project_id': projectId,
    };
}

/// A task's own workspace, else where its conversation works.
final class FileScopeTask extends FileScope {
  const FileScopeTask({
    required this.taskId,
  });

  final String taskId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'task',
      'task_id': taskId,
    };
}

/// The board member of the session starts (`on`) or stops following a ticket
/// (`ticket.follow`, feature `ticket_follow`).
final class FollowTicket {
  const FollowTicket({
    required this.on,
    required this.ticketId,
  });

  final bool on;
  final String ticketId;

  factory FollowTicket.fromJson(Map<String, dynamic> json) => FollowTicket(
    on: (json['on'] as bool),
    ticketId: (json['ticket_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'on': on,
    'ticket_id': ticketId,
  };
}

/// One board member who follows a ticket (`ticket.followers`, feature `ticket_follow`).
final class FollowerView {
  const FollowerView({
    required this.disabled,
    required this.followedAtMs,
    required this.reason,
    required this.userId,
    required this.username,
  });

  /// A disabled board member stays listed but gets no notifications.
  final bool disabled;
  final int followedAtMs;
  /// Why they follow: `created`, `responsible`, `mentioned` or `manual`. Text,
  /// not an enum, so a new reason never breaks an older client.
  final String reason;
  final String userId;
  final String username;

  factory FollowerView.fromJson(Map<String, dynamic> json) => FollowerView(
    disabled: (json['disabled'] as bool),
    followedAtMs: (json['followed_at_ms'] as num).toInt(),
    reason: (json['reason'] as String),
    userId: (json['user_id'] as String),
    username: (json['username'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'disabled': disabled,
    'followed_at_ms': followedAtMs,
    'reason': reason,
    'user_id': userId,
    'username': username,
  };
}

/// Remove from all future context and the search index; content is erased.
final class ForgetMemory {
  const ForgetMemory({
    required this.memoryId,
  });

  final String memoryId;

  factory ForgetMemory.fromJson(Map<String, dynamic> json) => ForgetMemory(
    memoryId: (json['memory_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'memory_id': memoryId,
  };
}

/// `guideline.affected`: who a change reaches, for "applies from the next run
/// of N employees" (B6). A run gets it at its next start; a conversation whose
/// session had another version starts a new session then.
final class GuidelineAffected {
  const GuidelineAffected({
    required this.conversations,
    required this.employees,
    required this.guidelineId,
    required this.projects,
  });

  /// Conversations whose current session holds a version of it (they start a new session).
  final int conversations;
  /// Active employees that get it: linked themselves, or working in a linked project.
  final int employees;
  final String guidelineId;
  /// Linked projects that are not archived.
  final int projects;

  factory GuidelineAffected.fromJson(Map<String, dynamic> json) => GuidelineAffected(
    conversations: (json['conversations'] as num).toInt(),
    employees: (json['employees'] as num).toInt(),
    guidelineId: (json['guideline_id'] as String),
    projects: (json['projects'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversations': conversations,
    'employees': employees,
    'guideline_id': guidelineId,
    'projects': projects,
  };
}

/// `guideline.get`: the guideline as the list shows it, plus one version with
/// its text (the current one unless a `version` was asked).
final class GuidelineDetail {
  const GuidelineDetail({
    required this.guideline,
    required this.version,
  });

  final GuidelineView guideline;
  final GuidelineVersionView version;

  factory GuidelineDetail.fromJson(Map<String, dynamic> json) => GuidelineDetail(
    guideline: GuidelineView.fromJson(json['guideline'] as Map<String, dynamic>),
    version: GuidelineVersionView.fromJson(json['version'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'guideline': guideline.toJson(),
    'version': version.toJson(),
  };
}

/// The level a guideline has in a run's block (B3: employee first, then project;
/// a guideline linked to both is in it once, at project level).
enum GuidelineLevel {
  employee('employee'),
  project('project');

  const GuidelineLevel(this.wire);

  final String wire;

  static GuidelineLevel fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown GuidelineLevel value: $value'),
      );

  String toJson() => wire;
}

/// A proposal from `propose_guideline` (`guideline.proposals`).
final class GuidelineProposalView {
  const GuidelineProposalView({
    this.baseVersion,
    required this.createdAtMs,
    this.decidedAtMs,
    this.decidedBy,
    this.decisionNote,
    this.evidence = const [],
    this.guidelineId,
    required this.id,
    this.originConversationId,
    required this.proposedBy,
    required this.reason,
    this.runId,
    required this.status,
    this.suggestedLinks = const [],
    required this.text,
    required this.title,
  });

  /// The version the proposal rests on (None for a new guideline).
  final int? baseVersion;
  final int createdAtMs;
  final int? decidedAtMs;
  final UserRef? decidedBy;
  final String? decisionNote;
  final List<ProposalEvidence> evidence;
  /// None: a new guideline.
  final String? guidelineId;
  final String id;
  final String? originConversationId;
  final String proposedBy;
  final String reason;
  final String? runId;
  final ProposalStatus status;
  /// For a new guideline: the links the proposer suggests (themselves and the
  /// run's project by default); the board member may change them.
  final List<GuidelineTarget> suggestedLinks;
  final String text;
  final String title;

  factory GuidelineProposalView.fromJson(Map<String, dynamic> json) => GuidelineProposalView(
    baseVersion: (json['base_version'] == null ? null : (json['base_version'] as num).toInt()),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    decidedAtMs: (json['decided_at_ms'] == null ? null : (json['decided_at_ms'] as num).toInt()),
    decidedBy: (json['decided_by'] == null ? null : UserRef.fromJson(json['decided_by'] as Map<String, dynamic>)),
    decisionNote: (json['decision_note'] == null ? null : (json['decision_note'] as String)),
    evidence: (json['evidence'] == null ? const [] : (json['evidence'] as List<dynamic>).map((x0) => ProposalEvidence.fromJson(x0 as Map<String, dynamic>)).toList()),
    guidelineId: (json['guideline_id'] == null ? null : (json['guideline_id'] as String)),
    id: (json['id'] as String),
    originConversationId: (json['origin_conversation_id'] == null ? null : (json['origin_conversation_id'] as String)),
    proposedBy: (json['proposed_by'] as String),
    reason: (json['reason'] as String),
    runId: (json['run_id'] == null ? null : (json['run_id'] as String)),
    status: ProposalStatus.fromJson(json['status'] as String),
    suggestedLinks: (json['suggested_links'] == null ? const [] : (json['suggested_links'] as List<dynamic>).map((x0) => GuidelineTarget.fromJson(x0 as Map<String, dynamic>)).toList()),
    text: (json['text'] as String),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (baseVersion != null) 'base_version': baseVersion,
    'created_at_ms': createdAtMs,
    if (decidedAtMs != null) 'decided_at_ms': decidedAtMs,
    if (decidedBy != null) 'decided_by': decidedBy?.toJson(),
    if (decisionNote != null) 'decision_note': decisionNote,
    'evidence': evidence.map((x0) => x0.toJson()).toList(),
    if (guidelineId != null) 'guideline_id': guidelineId,
    'id': id,
    if (originConversationId != null) 'origin_conversation_id': originConversationId,
    'proposed_by': proposedBy,
    'reason': reason,
    if (runId != null) 'run_id': runId,
    'status': status.toJson(),
    'suggested_links': suggestedLinks.map((x0) => x0.toJson()).toList(),
    'text': text,
    'title': title,
  };
}

/// `guideline.proposals`: pending first, then the latest decisions.
final class GuidelineProposals {
  const GuidelineProposals({
    required this.proposals,
  });

  final List<GuidelineProposalView> proposals;

  factory GuidelineProposals.fromJson(Map<String, dynamic> json) => GuidelineProposals(
    proposals: (json['proposals'] as List<dynamic>).map((x0) => GuidelineProposalView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'proposals': proposals.map((x0) => x0.toJson()).toList(),
  };
}

/// How a version came about.
enum GuidelineSource {
  created('created'),
  edited('edited'),
  /// An approved `propose_guideline`.
  proposal('proposal'),
  /// `guideline.restore_version`: a copy of an older version as the newest.
  restored('restored'),
  /// The starter set (B7).
  seed('seed');

  const GuidelineSource(this.wire);

  final String wire;

  static GuidelineSource fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown GuidelineSource value: $value'),
      );

  String toJson() => wire;
}

/// What a guideline is linked to: an employee or a project, never both (B2).
sealed class GuidelineTarget {
  const GuidelineTarget();

  factory GuidelineTarget.fromJson(Map<String, dynamic> json) {
    final tag = json['kind'] as String;
    switch (tag) {
      case 'employee':
        return GuidelineTargetEmployee(
          id: (json['id'] as String),
        );
      case 'project':
        return GuidelineTargetProject(
          id: (json['id'] as String),
        );
      default:
        throw FormatException('Unknown GuidelineTarget kind: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

final class GuidelineTargetEmployee extends GuidelineTarget {
  const GuidelineTargetEmployee({
    required this.id,
  });

  final String id;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'employee',
      'id': id,
    };
}

final class GuidelineTargetProject extends GuidelineTarget {
  const GuidelineTargetProject({
    required this.id,
  });

  final String id;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'project',
      'id': id,
    };
}

/// `guideline_too_large` (B4): a write would give one combination of an
/// employee and a project (or a project alone, or an employee without a
/// project) more than `limit` characters of guidelines. The first combination
/// that overflows; nothing was saved.
final class GuidelineTooLarge {
  const GuidelineTooLarge({
    required this.chars,
    this.employeeId,
    required this.limit,
    this.projectId,
  });

  final int chars;
  final String? employeeId;
  final int limit;
  final String? projectId;

  factory GuidelineTooLarge.fromJson(Map<String, dynamic> json) => GuidelineTooLarge(
    chars: (json['chars'] as num).toInt(),
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    limit: (json['limit'] as num).toInt(),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'chars': chars,
    if (employeeId != null) 'employee_id': employeeId,
    'limit': limit,
    if (projectId != null) 'project_id': projectId,
  };
}

/// One version with its text (`guideline.get`, `guideline.versions`).
final class GuidelineVersionView {
  const GuidelineVersionView({
    this.by,
    required this.createdAtMs,
    required this.guidelineId,
    this.proposalId,
    this.restoredFrom,
    required this.source,
    required this.text,
    required this.title,
    required this.version,
  });

  /// None: the seed.
  final UserRef? by;
  final int createdAtMs;
  final String guidelineId;
  final String? proposalId;
  final int? restoredFrom;
  final GuidelineSource source;
  final String text;
  final String title;
  final int version;

  factory GuidelineVersionView.fromJson(Map<String, dynamic> json) => GuidelineVersionView(
    by: (json['by'] == null ? null : UserRef.fromJson(json['by'] as Map<String, dynamic>)),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    guidelineId: (json['guideline_id'] as String),
    proposalId: (json['proposal_id'] == null ? null : (json['proposal_id'] as String)),
    restoredFrom: (json['restored_from'] == null ? null : (json['restored_from'] as num).toInt()),
    source: GuidelineSource.fromJson(json['source'] as String),
    text: (json['text'] as String),
    title: (json['title'] as String),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (by != null) 'by': by?.toJson(),
    'created_at_ms': createdAtMs,
    'guideline_id': guidelineId,
    if (proposalId != null) 'proposal_id': proposalId,
    if (restoredFrom != null) 'restored_from': restoredFrom,
    'source': source.toJson(),
    'text': text,
    'title': title,
    'version': version,
  };
}

/// `guideline.versions`: every version, newest first.
final class GuidelineVersions {
  const GuidelineVersions({
    required this.guidelineId,
    required this.versions,
  });

  final String guidelineId;
  final List<GuidelineVersionView> versions;

  factory GuidelineVersions.fromJson(Map<String, dynamic> json) => GuidelineVersions(
    guidelineId: (json['guideline_id'] as String),
    versions: (json['versions'] as List<dynamic>).map((x0) => GuidelineVersionView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'guideline_id': guidelineId,
    'versions': versions.map((x0) => x0.toJson()).toList(),
  };
}

/// One guideline in the library list (`guideline.list`), without its text.
final class GuidelineView {
  const GuidelineView({
    this.archived,
    required this.chars,
    required this.id,
    required this.links,
    this.openProposals = 0,
    required this.title,
    required this.updatedAtMs,
    this.updatedBy,
    required this.version,
  });

  final bool? archived;
  /// Characters of the current text.
  final int chars;
  final String id;
  final List<GuidelineTarget> links;
  /// Pending proposals for this guideline.
  final int openProposals;
  final String title;
  final int updatedAtMs;
  final UserRef? updatedBy;
  /// The current version (the envelope's `expected_version`).
  final int version;

  factory GuidelineView.fromJson(Map<String, dynamic> json) => GuidelineView(
    archived: (json['archived'] == null ? null : (json['archived'] as bool)),
    chars: (json['chars'] as num).toInt(),
    id: (json['id'] as String),
    links: (json['links'] as List<dynamic>).map((x0) => GuidelineTarget.fromJson(x0 as Map<String, dynamic>)).toList(),
    openProposals: (json['open_proposals'] == null ? 0 : (json['open_proposals'] as num).toInt()),
    title: (json['title'] as String),
    updatedAtMs: (json['updated_at_ms'] as num).toInt(),
    updatedBy: (json['updated_by'] == null ? null : UserRef.fromJson(json['updated_by'] as Map<String, dynamic>)),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (archived != null) 'archived': archived,
    'chars': chars,
    'id': id,
    'links': links.map((x0) => x0.toJson()).toList(),
    'open_proposals': openProposals,
    'title': title,
    'updated_at_ms': updatedAtMs,
    if (updatedBy != null) 'updated_by': updatedBy?.toJson(),
    'version': version,
  };
}

/// `guideline.list`: the library, without texts. Archived ones only with `archived`.
final class GuidelinesSnapshot {
  const GuidelinesSnapshot({
    required this.guidelines,
  });

  final List<GuidelineView> guidelines;

  factory GuidelinesSnapshot.fromJson(Map<String, dynamic> json) => GuidelinesSnapshot(
    guidelines: (json['guidelines'] as List<dynamic>).map((x0) => GuidelineView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'guidelines': guidelines.map((x0) => x0.toJson()).toList(),
  };
}

final class HealthResult {
  const HealthResult({
    required this.activeJobs,
    required this.daemonVersion,
    required this.hostId,
    required this.lastSeq,
    required this.pid,
    required this.startedAtMs,
  });

  final int activeJobs;
  final String daemonVersion;
  final String hostId;
  final int lastSeq;
  final int pid;
  final int startedAtMs;

  factory HealthResult.fromJson(Map<String, dynamic> json) => HealthResult(
    activeJobs: (json['active_jobs'] as num).toInt(),
    daemonVersion: (json['daemon_version'] as String),
    hostId: (json['host_id'] as String),
    lastSeq: (json['last_seq'] as num).toInt(),
    pid: (json['pid'] as num).toInt(),
    startedAtMs: (json['started_at_ms'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'active_jobs': activeJobs,
    'daemon_version': daemonVersion,
    'host_id': hostId,
    'last_seq': lastSeq,
    'pid': pid,
    'started_at_ms': startedAtMs,
  };
}

final class HelloParams {
  const HelloParams({
    this.accepts = const [],
    required this.clientName,
    required this.clientVersion,
    this.sessionToken,
  });

  /// What this client can take that older clients cannot (`client_feature`).
  /// Servers without such a feature ignore the name.
  final List<String> accepts;
  final String clientName;
  final String clientVersion;
  /// The session this device got from `user.login` (or the first
  /// `user.create`) for this server: valid, it makes its board member this
  /// connection's (`HelloResult::user`); unknown, revoked or expired, the
  /// hello still works without a board member and the client shows its
  /// sign-in screen. Ignored for apps. Never printed.
  final String? sessionToken;

  factory HelloParams.fromJson(Map<String, dynamic> json) => HelloParams(
    accepts: (json['accepts'] == null ? const [] : (json['accepts'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    clientName: (json['client_name'] as String),
    clientVersion: (json['client_version'] as String),
    sessionToken: (json['session_token'] == null ? null : (json['session_token'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'accepts': accepts,
    'client_name': clientName,
    'client_version': clientVersion,
    if (sessionToken != null) 'session_token': sessionToken,
  };
}

final class HelloResult {
  const HelloResult({
    this.boardMembersExist = false,
    required this.capabilities,
    this.claudeLogin,
    this.clientId,
    required this.daemonVersion,
    required this.environment,
    this.features = const [],
    required this.hostId,
    required this.hostName,
    this.mustChangePassword,
    required this.os,
    required this.protocol,
    required this.remote,
    this.user,
  });

  /// The server has at least one board member; false: the onboarding
  /// ("Create the first board member") is all a client shows.
  final bool boardMembersExist;
  final List<Capability> capabilities;
  /// The session's own Claude login; none without a session.
  final ClaudeLoginView? claudeLogin;
  /// The paired device behind a remote connection.
  final String? clientId;
  final String daemonVersion;
  /// `dev` or `production`; a client never talks to the other environment.
  final String environment;
  /// Additions to protocol 3.0 this server has (see `FEATURES`); a client hides
  /// what is missing, since an unknown method is not a clean error. Older servers leave it out.
  final List<String> features;
  final String hostId;
  /// The server's computer name.
  final String hostName;
  /// The session's board member must change a temporary password first.
  final bool? mustChangePassword;
  /// The server's OS: `windows`, `macos` or `linux`.
  final String os;
  final int protocol;
  /// This connection came in over the network.
  final bool remote;
  /// The board member of this connection's session (`HelloParams::session_token`
  /// when it is valid); none: the client shows its sign-in screen (or the
  /// onboarding without board members).
  final UserView? user;

  factory HelloResult.fromJson(Map<String, dynamic> json) => HelloResult(
    boardMembersExist: (json['board_members_exist'] == null ? false : (json['board_members_exist'] as bool)),
    capabilities: (json['capabilities'] as List<dynamic>).map((x0) => Capability.fromJson(x0 as String)).toList(),
    claudeLogin: (json['claude_login'] == null ? null : ClaudeLoginView.fromJson(json['claude_login'] as Map<String, dynamic>)),
    clientId: (json['client_id'] == null ? null : (json['client_id'] as String)),
    daemonVersion: (json['daemon_version'] as String),
    environment: (json['environment'] as String),
    features: (json['features'] == null ? const [] : (json['features'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    hostId: (json['host_id'] as String),
    hostName: (json['host_name'] as String),
    mustChangePassword: (json['must_change_password'] == null ? null : (json['must_change_password'] as bool)),
    os: (json['os'] as String),
    protocol: (json['protocol'] as num).toInt(),
    remote: (json['remote'] as bool),
    user: (json['user'] == null ? null : UserView.fromJson(json['user'] as Map<String, dynamic>)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'board_members_exist': boardMembersExist,
    'capabilities': capabilities.map((x0) => x0.toJson()).toList(),
    if (claudeLogin != null) 'claude_login': claudeLogin?.toJson(),
    if (clientId != null) 'client_id': clientId,
    'daemon_version': daemonVersion,
    'environment': environment,
    'features': features,
    'host_id': hostId,
    'host_name': hostName,
    if (mustChangePassword != null) 'must_change_password': mustChangePassword,
    'os': os,
    'protocol': protocol,
    'remote': remote,
    if (user != null) 'user': user?.toJson(),
  };
}

/// One finished ticket in the History.
final class HistoryEntry {
  const HistoryEntry({
    required this.doneAtMs,
    required this.number,
    this.ownerEmployeeId,
    this.ownerName,
    required this.projectId,
    required this.projectName,
    this.summary,
    required this.summaryState,
    required this.ticketId,
    required this.title,
  });

  /// When the ticket went to Done (for a quick task: after its merge).
  final int doneAtMs;
  final int number;
  /// The ticket's owner (an archived employee keeps its name); none: "—".
  final String? ownerEmployeeId;
  final String? ownerName;
  final String projectId;
  final String projectName;
  /// What the ticket delivered (through the vault redactor), when `summary_state` is `ready`.
  final String? summary;
  final HistorySummaryState summaryState;
  final String ticketId;
  final String title;

  factory HistoryEntry.fromJson(Map<String, dynamic> json) => HistoryEntry(
    doneAtMs: (json['done_at_ms'] as num).toInt(),
    number: (json['number'] as num).toInt(),
    ownerEmployeeId: (json['owner_employee_id'] == null ? null : (json['owner_employee_id'] as String)),
    ownerName: (json['owner_name'] == null ? null : (json['owner_name'] as String)),
    projectId: (json['project_id'] as String),
    projectName: (json['project_name'] as String),
    summary: (json['summary'] == null ? null : (json['summary'] as String)),
    summaryState: HistorySummaryState.fromJson(json['summary_state'] as String),
    ticketId: (json['ticket_id'] as String),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'done_at_ms': doneAtMs,
    'number': number,
    if (ownerEmployeeId != null) 'owner_employee_id': ownerEmployeeId,
    if (ownerName != null) 'owner_name': ownerName,
    'project_id': projectId,
    'project_name': projectName,
    if (summary != null) 'summary': summary,
    'summary_state': summaryState.toJson(),
    'ticket_id': ticketId,
    'title': title,
  };
}

/// `ticket.history`: finished tickets, newest first.
final class HistoryPage {
  const HistoryPage({
    required this.asOfSeq,
    required this.entries,
    this.hasMore = false,
  });

  final int asOfSeq;
  final List<HistoryEntry> entries;
  /// There are older rows: ask again with the last row's `done_at_ms` and `number`.
  final bool hasMore;

  factory HistoryPage.fromJson(Map<String, dynamic> json) => HistoryPage(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    entries: (json['entries'] as List<dynamic>).map((x0) => HistoryEntry.fromJson(x0 as Map<String, dynamic>)).toList(),
    hasMore: (json['has_more'] == null ? false : (json['has_more'] as bool)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'entries': entries.map((x0) => x0.toJson()).toList(),
    'has_more': hasMore,
  };
}

/// Where a row's summary stands, as the History shows it.
enum HistorySummaryState {
  /// The ticket went to Done before the History existed and its project memory
  /// has no summary of it ("No summary (finished before History)").
  beforeHistory('before_history'),
  /// It failed or gave nothing ("Summary not available"); a board member may try again.
  notAvailable('not_available'),
  /// `summary` holds the text.
  ready('ready'),
  /// The extraction is queued or running ("Summarizing…").
  summarizing('summarizing');

  const HistorySummaryState(this.wire);

  final String wire;

  static HistorySummaryState fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown HistorySummaryState value: $value'),
      );

  String toJson() => wire;
}

/// What a host write answers.
sealed class HostResult {
  const HostResult();

  factory HostResult.fromJson(Map<String, dynamic> json) {
    final tag = json['type'] as String;
    switch (tag) {
      case 'auto_allow_setting':
        return HostResultAutoAllowSetting(value: AutoAllowSetting.fromJson(json as Map<String, dynamic>));
      case 'backups':
        return HostResultBackups(value: BackupsSnapshot.fromJson(json as Map<String, dynamic>));
      case 'browser':
        return HostResultBrowser(value: BrowserSetting.fromJson(json as Map<String, dynamic>));
      case 'claude_login':
        return HostResultClaudeLogin(value: ClaudeLoginView.fromJson(json as Map<String, dynamic>));
      case 'claude_login_link':
        return HostResultClaudeLoginLink(
          linkId: (json['link_id'] as String),
        );
      case 'code':
        return HostResultCode(value: CodeView.fromJson(json as Map<String, dynamic>));
      case 'compaction_setting':
        return HostResultCompactionSetting(value: CompactionSetting.fromJson(json as Map<String, dynamic>));
      case 'count':
        return HostResultCount(
          count: (json['count'] as num).toInt(),
        );
      case 'dir_listing':
        return HostResultDirListing(value: DirListing.fromJson(json as Map<String, dynamic>));
      case 'done':
        return const HostResultDone();
      case 'exported':
        return HostResultExported(
          jsonPath: (json['json_path'] as String),
          markdownPath: (json['markdown_path'] as String),
        );
      case 'load':
        return HostResultLoad(value: LoadSnapshot.fromJson(json as Map<String, dynamic>));
      case 'management_mode_setting':
        return HostResultManagementModeSetting(value: ManagementModeSetting.fromJson(json as Map<String, dynamic>));
      case 'mcp_always_allow':
        return HostResultMcpAlwaysAllow(value: McpAlwaysAllow.fromJson(json as Map<String, dynamic>));
      case 'mcp_login':
        return HostResultMcpLogin(value: McpLogin.fromJson(json as Map<String, dynamic>));
      case 'mcp_servers':
        return HostResultMcpServers(value: McpServers.fromJson(json as Map<String, dynamic>));
      case 'pairing_code':
        return HostResultPairingCode(value: PairingCode.fromJson(json as Map<String, dynamic>));
      case 'power':
        return HostResultPower(value: PowerState.fromJson(json as Map<String, dynamic>));
      case 'remote':
        return HostResultRemote(value: RemoteStatus.fromJson(json as Map<String, dynamic>));
      case 'shutting_down':
        return const HostResultShuttingDown();
      case 'trace_setting':
        return HostResultTraceSetting(value: TraceSetting.fromJson(json as Map<String, dynamic>));
      case 'update':
        return HostResultUpdate(value: UpdateStatus.fromJson(json as Map<String, dynamic>));
      case 'user':
        return HostResultUser(value: UserView.fromJson(json as Map<String, dynamic>));
      case 'vault_item':
        return HostResultVaultItem(value: VaultItemView.fromJson(json as Map<String, dynamic>));
      default:
        throw FormatException('Unknown HostResult type: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

final class HostResultAutoAllowSetting extends HostResult {
  const HostResultAutoAllowSetting({
    required this.value,
  });

  final AutoAllowSetting value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'auto_allow_setting',
      ...value.toJson(),
    };
}

final class HostResultBackups extends HostResult {
  const HostResultBackups({
    required this.value,
  });

  final BackupsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'backups',
      ...value.toJson(),
    };
}

final class HostResultBrowser extends HostResult {
  const HostResultBrowser({
    required this.value,
  });

  final BrowserSetting value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'browser',
      ...value.toJson(),
    };
}

/// `claude_login.cancel` / `claude_login.unlink`: the login as it is now.
final class HostResultClaudeLogin extends HostResult {
  const HostResultClaudeLogin({
    required this.value,
  });

  final ClaudeLoginView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'claude_login',
      ...value.toJson(),
    };
}

/// `claude_login.link`: the link that runs; its prompts follow as
/// `ServerMessage::ClaudeLoginPrompt` with this id.
final class HostResultClaudeLoginLink extends HostResult {
  const HostResultClaudeLoginLink({
    required this.linkId,
  });

  final String linkId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'claude_login_link',
      'link_id': linkId,
    };
}

/// `code.*`: the project's git state after the write.
final class HostResultCode extends HostResult {
  const HostResultCode({
    required this.value,
  });

  final CodeView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'code',
      ...value.toJson(),
    };
}

final class HostResultCompactionSetting extends HostResult {
  const HostResultCompactionSetting({
    required this.value,
  });

  final CompactionSetting value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'compaction_setting',
      ...value.toJson(),
    };
}

/// Notifications changed.
final class HostResultCount extends HostResult {
  const HostResultCount({
    required this.count,
  });

  final int count;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'count',
      'count': count,
    };
}

final class HostResultDirListing extends HostResult {
  const HostResultDirListing({
    required this.value,
  });

  final DirListing value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'dir_listing',
      ...value.toJson(),
    };
}

final class HostResultDone extends HostResult {
  const HostResultDone();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'done',
    };
}

final class HostResultExported extends HostResult {
  const HostResultExported({
    required this.jsonPath,
    required this.markdownPath,
  });

  final String jsonPath;
  final String markdownPath;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'exported',
      'json_path': jsonPath,
      'markdown_path': markdownPath,
    };
}

final class HostResultLoad extends HostResult {
  const HostResultLoad({
    required this.value,
  });

  final LoadSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'load',
      ...value.toJson(),
    };
}

final class HostResultManagementModeSetting extends HostResult {
  const HostResultManagementModeSetting({
    required this.value,
  });

  final ManagementModeSetting value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'management_mode_setting',
      ...value.toJson(),
    };
}

final class HostResultMcpAlwaysAllow extends HostResult {
  const HostResultMcpAlwaysAllow({
    required this.value,
  });

  final McpAlwaysAllow value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'mcp_always_allow',
      ...value.toJson(),
    };
}

final class HostResultMcpLogin extends HostResult {
  const HostResultMcpLogin({
    required this.value,
  });

  final McpLogin value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'mcp_login',
      ...value.toJson(),
    };
}

final class HostResultMcpServers extends HostResult {
  const HostResultMcpServers({
    required this.value,
  });

  final McpServers value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'mcp_servers',
      ...value.toJson(),
    };
}

/// A replayed receipt has empty codes.
final class HostResultPairingCode extends HostResult {
  const HostResultPairingCode({
    required this.value,
  });

  final PairingCode value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'pairing_code',
      ...value.toJson(),
    };
}

final class HostResultPower extends HostResult {
  const HostResultPower({
    required this.value,
  });

  final PowerState value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'power',
      ...value.toJson(),
    };
}

final class HostResultRemote extends HostResult {
  const HostResultRemote({
    required this.value,
  });

  final RemoteStatus value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'remote',
      ...value.toJson(),
    };
}

final class HostResultShuttingDown extends HostResult {
  const HostResultShuttingDown();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'shutting_down',
    };
}

final class HostResultTraceSetting extends HostResult {
  const HostResultTraceSetting({
    required this.value,
  });

  final TraceSetting value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'trace_setting',
      ...value.toJson(),
    };
}

final class HostResultUpdate extends HostResult {
  const HostResultUpdate({
    required this.value,
  });

  final UpdateStatus value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'update',
      ...value.toJson(),
    };
}

/// `user.disable`: the board member as it is now.
final class HostResultUser extends HostResult {
  const HostResultUser({
    required this.value,
  });

  final UserView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'user',
      ...value.toJson(),
    };
}

/// `vault.create` / `vault.update` / `vault.archive`: the item as it is now.
/// A replayed receipt has no value: read it with `vault.list` or `vault.reveal`.
final class HostResultVaultItem extends HostResult {
  const HostResultVaultItem({
    required this.value,
  });

  final VaultItemView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'vault_item',
      ...value.toJson(),
    };
}

/// Writes only the host can do. Secrets in parameters are never printed.
sealed class HostWrite {
  const HostWrite();

  factory HostWrite.fromJson(Map<String, dynamic> json) {
    final tag = json['method'] as String;
    switch (tag) {
      case 'auto_allow.set_enabled':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteAutoAllowSetEnabled(
          enabled: (src['enabled'] as bool),
        );
        }
      case 'backup.create':
        return const HostWriteBackupCreate();
      case 'backup.restore':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteBackupRestore(
          id: (src['id'] as String),
        );
        }
      case 'backup.resume':
        return const HostWriteBackupResume();
      case 'browser.set':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteBrowserSet(
          choice: BrowserChoice.fromJson(src['choice'] as String),
        );
        }
      case 'browser.set_backend':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteBrowserSetBackend(
          backend: BrowserBackend.fromJson(src['backend'] as String),
        );
        }
      case 'claude_login.cancel':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteClaudeLoginCancel(
          linkId: (src['link_id'] as String),
        );
        }
      case 'claude_login.link':
        return const HostWriteClaudeLoginLink();
      case 'claude_login.unlink':
        return const HostWriteClaudeLoginUnlink();
      case 'code.delete_branch':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteCodeDeleteBranch(
          branch: (src['branch'] as String),
          force: (src['force'] == null ? false : (src['force'] as bool)),
          projectId: (src['project_id'] as String),
        );
        }
      case 'code.fetch':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteCodeFetch(
          projectId: (src['project_id'] as String),
        );
        }
      case 'code.merge':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteCodeMerge(
          branch: (src['branch'] as String),
          projectId: (src['project_id'] as String),
        );
        }
      case 'code.prune_merged':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteCodePruneMerged(
          projectId: (src['project_id'] as String),
        );
        }
      case 'code.remove_worktree':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteCodeRemoveWorktree(
          path: (src['path'] as String),
          projectId: (src['project_id'] as String),
        );
        }
      case 'compaction.set_threshold':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteCompactionSetThreshold(
          thresholdTokens: (src['threshold_tokens'] as num).toInt(),
        );
        }
      case 'fs.create_dir':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteFsCreateDir(
          name: (src['name'] as String),
          parent: (src['parent'] as String),
        );
        }
      case 'load.set_limits':
        return HostWriteLoadSetLimits(params: LoadLimitsUpdate.fromJson(json['params'] as Map<String, dynamic>));
      case 'management_mode.set_enabled':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteManagementModeSetEnabled(
          enabled: (src['enabled'] as bool),
        );
        }
      case 'mcp.add':
        return HostWriteMcpAdd(params: McpAddParams.fromJson(json['params'] as Map<String, dynamic>));
      case 'mcp.login_complete':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteMcpLoginComplete(
          loginId: (src['login_id'] as String),
          redirectUrl: (src['redirect_url'] as String),
        );
        }
      case 'mcp.login_start':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteMcpLoginStart(
          name: (src['name'] as String),
        );
        }
      case 'mcp.logout':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteMcpLogout(
          name: (src['name'] as String),
        );
        }
      case 'mcp.remove':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteMcpRemove(
          name: (src['name'] as String),
        );
        }
      case 'mcp.set':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteMcpSet(
          enabled: (src['enabled'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
        );
        }
      case 'mcp_always_allow.remove':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteMcpAlwaysAllowRemove(
          tool: (src['tool'] as String),
        );
        }
      case 'notification.dismiss':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteNotificationDismiss(
          id: (src['id'] as String),
        );
        }
      case 'notification.dismiss_all':
        return const HostWriteNotificationDismissAll();
      case 'notification.mark_all_read':
        return const HostWriteNotificationMarkAllRead();
      case 'notification.mark_read':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteNotificationMarkRead(
          id: (src['id'] as String),
        );
        }
      case 'power.cancel':
        return const HostWritePowerCancel();
      case 'power.shutdown':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWritePowerShutdown(
          when: ShutdownWhen.fromJson(src['when'] as String),
        );
        }
      case 'project.export':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteProjectExport(
          projectId: (src['project_id'] as String),
        );
        }
      case 'push.configure':
        return HostWritePushConfigure(params: PushConfig.fromJson(json['params'] as Map<String, dynamic>));
      case 'push.test':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWritePushTest(
          clientId: (src['client_id'] as String),
        );
        }
      case 'remote.decide_pairing':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteRemoteDecidePairing(
          approve: (src['approve'] as bool),
          owner: (src['owner'] as bool),
          pairingId: (src['pairing_id'] as String),
        );
        }
      case 'remote.disable':
        return const HostWriteRemoteDisable();
      case 'remote.enable':
        return const HostWriteRemoteEnable();
      case 'remote.revoke':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteRemoteRevoke(
          clientId: (src['client_id'] as String),
        );
        }
      case 'remote.start_pairing':
        return const HostWriteRemoteStartPairing();
      case 'secret.set':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteSecretSet(
          connectionId: (src['connection_id'] as String),
          secret: (src['secret'] as String),
        );
        }
      case 'server.restart':
        return const HostWriteServerRestart();
      case 'session.revoke':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteSessionRevoke(
          sessionId: (src['session_id'] as String),
        );
        }
      case 'trace.set_enabled':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteTraceSetEnabled(
          enabled: (src['enabled'] as bool),
        );
        }
      case 'update.apply':
        return const HostWriteUpdateApply();
      case 'update.apply_app':
        return const HostWriteUpdateApplyApp();
      case 'update.prepare':
        return const HostWriteUpdatePrepare();
      case 'update.prepare_app':
        return const HostWriteUpdatePrepareApp();
      case 'user.create':
        return HostWriteUserCreate(params: UserCreateParams.fromJson(json['params'] as Map<String, dynamic>));
      case 'user.disable':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteUserDisable(
          userId: (src['user_id'] as String),
        );
        }
      case 'user.set_password':
        return HostWriteUserSetPassword(params: SetPasswordParams.fromJson(json['params'] as Map<String, dynamic>));
      case 'vault.archive':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return HostWriteVaultArchive(
          itemId: (src['item_id'] as String),
        );
        }
      case 'vault.create':
        return HostWriteVaultCreate(params: VaultCreateParams.fromJson(json['params'] as Map<String, dynamic>));
      case 'vault.update':
        return HostWriteVaultUpdate(params: VaultUpdateParams.fromJson(json['params'] as Map<String, dynamic>));
      default:
        throw FormatException('Unknown HostWrite method: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

/// Auto-allow of the safe read-only tools on or off, from the next tool call. On by default.
final class HostWriteAutoAllowSetEnabled extends HostWrite {
  const HostWriteAutoAllowSetEnabled({
    required this.enabled,
  });

  final bool enabled;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'auto_allow.set_enabled',
      'params': <String, dynamic>{
      'enabled': enabled,
    },
    };
}

final class HostWriteBackupCreate extends HostWrite {
  const HostWriteBackupCreate();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'backup.create',
    };
}

/// Restore at the next start (the service stops now); work stays paused until resumed.
final class HostWriteBackupRestore extends HostWrite {
  const HostWriteBackupRestore({
    required this.id,
  });

  final String id;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'backup.restore',
      'params': <String, dynamic>{
      'id': id,
    },
    };
}

final class HostWriteBackupResume extends HostWrite {
  const HostWriteBackupResume();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'backup.resume',
    };
}

final class HostWriteBrowserSet extends HostWrite {
  const HostWriteBrowserSet({
    required this.choice,
  });

  final BrowserChoice choice;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'browser.set',
      'params': <String, dynamic>{
      'choice': choice.toJson(),
    },
    };
}

/// Claude in Chrome or Playwright, for every run that starts afterwards.
final class HostWriteBrowserSetBackend extends HostWrite {
  const HostWriteBrowserSetBackend({
    required this.backend,
  });

  final BrowserBackend backend;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'browser.set_backend',
      'params': <String, dynamic>{
      'backend': backend.toJson(),
    },
    };
}

/// Stop a running link of the session's own login. Answers `ClaudeLogin`.
final class HostWriteClaudeLoginCancel extends HostWrite {
  const HostWriteClaudeLoginCancel({
    required this.linkId,
  });

  final String linkId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'claude_login.cancel',
      'params': <String, dynamic>{
      'link_id': linkId,
    },
    };
}

/// Connect the session's own Claude login: the server starts `claude auth
/// login` for `<data>/logins/<user_id>/` and answers `ClaudeLoginLink`. What
/// Claude asks (the sign-in link, the code) comes to this connection only as
/// `ServerMessage::ClaudeLoginPrompt`; the code goes back with
/// `claude_login.input`. One link per board member at a time, 10 minutes.
final class HostWriteClaudeLoginLink extends HostWrite {
  const HostWriteClaudeLoginLink();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'claude_login.link',
    };
}

/// Disconnect the session's own Claude login (`claude auth logout`, its folder
/// removed). Answers `ClaudeLogin`.
final class HostWriteClaudeLoginUnlink extends HostWrite {
  const HostWriteClaudeLoginUnlink();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'claude_login.unlink',
    };
}

/// Delete a local branch (not main, not one checked out in a worktree, not one an
/// employee works on now). A branch with commits main lacks needs `force` (the
/// client asked the board member first: that work is gone). Answers `Code`.
final class HostWriteCodeDeleteBranch extends HostWrite {
  const HostWriteCodeDeleteBranch({
    required this.branch,
    this.force = false,
    required this.projectId,
  });

  final String branch;
  final bool force;
  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'code.delete_branch',
      'params': <String, dynamic>{
      'branch': branch,
      'force': force,
      'project_id': projectId,
    },
    };
}

/// `git fetch --prune` of the project's repository (feature `code_view`, ticket #77);
/// answers `HostResult::Code`. Never a prompt for credentials: a remote that wants
/// one fails with git's reason.
final class HostWriteCodeFetch extends HostWrite {
  const HostWriteCodeFetch({
    required this.projectId,
  });

  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'code.fetch',
      'params': <String, dynamic>{
      'project_id': projectId,
    },
    };
}

/// Merge a local branch into the project's main (`--no-ff`), in the project's folder.
/// Refused when the folder is not on main, main has uncommitted changes, is in another
/// merge, or the branch has nothing ahead; a conflict is aborted, so main stays as it
/// was (`validation` with git's reason and the conflicting files). Answers `Code`.
final class HostWriteCodeMerge extends HostWrite {
  const HostWriteCodeMerge({
    required this.branch,
    required this.projectId,
  });

  final String branch;
  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'code.merge',
      'params': <String, dynamic>{
      'branch': branch,
      'project_id': projectId,
    },
    };
}

/// Clean up merged: remove every clean worktree of a merged branch nobody works on,
/// then delete every merged branch without a worktree (never main), then
/// `git worktree prune`. Answers `Code`.
final class HostWriteCodePruneMerged extends HostWrite {
  const HostWriteCodePruneMerged({
    required this.projectId,
  });

  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'code.prune_merged',
      'params': <String, dynamic>{
      'project_id': projectId,
    },
    };
}

/// Remove a worktree (not the project's folder). Refused when it has uncommitted
/// changes or an employee works in it now. The branch stays. Answers `Code`.
final class HostWriteCodeRemoveWorktree extends HostWrite {
  const HostWriteCodeRemoveWorktree({
    required this.path,
    required this.projectId,
  });

  final String path;
  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'code.remove_worktree',
      'params': <String, dynamic>{
      'path': path,
      'project_id': projectId,
    },
    };
}

/// Above how many tokens of context a session is compacted before its next turn
/// (feature `compaction_setting`), from the next run on; 0: never.
final class HostWriteCompactionSetThreshold extends HostWrite {
  const HostWriteCompactionSetThreshold({
    required this.thresholdTokens,
  });

  final int thresholdTokens;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'compaction.set_threshold',
      'params': <String, dynamic>{
      'threshold_tokens': thresholdTokens,
    },
    };
}

/// A new folder `name` in the folder `parent`.
final class HostWriteFsCreateDir extends HostWrite {
  const HostWriteFsCreateDir({
    required this.name,
    required this.parent,
  });

  final String name;
  final String parent;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'fs.create_dir',
      'params': <String, dynamic>{
      'name': name,
      'parent': parent,
    },
    };
}

final class HostWriteLoadSetLimits extends HostWrite {
  const HostWriteLoadSetLimits({
    required this.params,
  });

  final LoadLimitsUpdate params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'load.set_limits',
      'params': params.toJson(),
    };
}

/// Management chooses its own task mode (feature `management_chooses_mode`), for every run
/// that starts afterwards. On by default.
final class HostWriteManagementModeSetEnabled extends HostWrite {
  const HostWriteManagementModeSetEnabled({
    required this.enabled,
  });

  final bool enabled;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'management_mode.set_enabled',
      'params': <String, dynamic>{
      'enabled': enabled,
    },
    };
}

final class HostWriteMcpAdd extends HostWrite {
  const HostWriteMcpAdd({
    required this.params,
  });

  final McpAddParams params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'mcp.add',
      'params': params.toJson(),
    };
}

/// Finishes a login with the address the browser ended on (`…/callback?code=…&state=…`),
/// for a browser that is not on the server's computer.
final class HostWriteMcpLoginComplete extends HostWrite {
  const HostWriteMcpLoginComplete({
    required this.loginId,
    required this.redirectUrl,
  });

  final String loginId;
  final String redirectUrl;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'mcp.login_complete',
      'params': <String, dynamic>{
      'login_id': loginId,
      'redirect_url': redirectUrl,
    },
    };
}

/// Starts an OAuth login for an http/sse server; answers `McpLogin` with the URL to open.
final class HostWriteMcpLoginStart extends HostWrite {
  const HostWriteMcpLoginStart({
    required this.name,
  });

  final String name;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'mcp.login_start',
      'params': <String, dynamic>{
      'name': name,
    },
    };
}

/// Forgets the server's tokens (keeps its client registration).
final class HostWriteMcpLogout extends HostWrite {
  const HostWriteMcpLogout({
    required this.name,
  });

  final String name;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'mcp.logout',
      'params': <String, dynamic>{
      'name': name,
    },
    };
}

final class HostWriteMcpRemove extends HostWrite {
  const HostWriteMcpRemove({
    required this.name,
  });

  final String name;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'mcp.remove',
      'params': <String, dynamic>{
      'name': name,
    },
    };
}

final class HostWriteMcpSet extends HostWrite {
  const HostWriteMcpSet({
    required this.enabled,
  });

  final List<String> enabled;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'mcp.set',
      'params': <String, dynamic>{
      'enabled': enabled,
    },
    };
}

/// Take a tool off the always-allow list: its next call asks again.
final class HostWriteMcpAlwaysAllowRemove extends HostWrite {
  const HostWriteMcpAlwaysAllowRemove({
    required this.tool,
  });

  final String tool;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'mcp_always_allow.remove',
      'params': <String, dynamic>{
      'tool': tool,
    },
    };
}

final class HostWriteNotificationDismiss extends HostWrite {
  const HostWriteNotificationDismiss({
    required this.id,
  });

  final String id;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'notification.dismiss',
      'params': <String, dynamic>{
      'id': id,
    },
    };
}

final class HostWriteNotificationDismissAll extends HostWrite {
  const HostWriteNotificationDismissAll();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'notification.dismiss_all',
    };
}

final class HostWriteNotificationMarkAllRead extends HostWrite {
  const HostWriteNotificationMarkAllRead();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'notification.mark_all_read',
    };
}

/// One row read, for this board member only (feature `personal_inbox`); answers
/// `Count`, 0 when there was nothing to read, so it can be repeated.
final class HostWriteNotificationMarkRead extends HostWrite {
  const HostWriteNotificationMarkRead({
    required this.id,
  });

  final String id;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'notification.mark_read',
      'params': <String, dynamic>{
      'id': id,
    },
    };
}

final class HostWritePowerCancel extends HostWrite {
  const HostWritePowerCancel();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'power.cancel',
    };
}

final class HostWritePowerShutdown extends HostWrite {
  const HostWritePowerShutdown({
    required this.when,
  });

  final ShutdownWhen when;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'power.shutdown',
      'params': <String, dynamic>{
      'when': when.toJson(),
    },
    };
}

/// Write a readable Markdown + JSON export of a project into its folder.
final class HostWriteProjectExport extends HostWrite {
  const HostWriteProjectExport({
    required this.projectId,
  });

  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'project.export',
      'params': <String, dynamic>{
      'project_id': projectId,
    },
    };
}

final class HostWritePushConfigure extends HostWrite {
  const HostWritePushConfigure({
    required this.params,
  });

  final PushConfig params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'push.configure',
      'params': params.toJson(),
    };
}

final class HostWritePushTest extends HostWrite {
  const HostWritePushTest({
    required this.clientId,
  });

  final String clientId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'push.test',
      'params': <String, dynamic>{
      'client_id': clientId,
    },
    };
}

/// Allow or refuse a device that entered the code. A computer that asks for
/// full access is only allowed with `owner` (the user confirmed it).
final class HostWriteRemoteDecidePairing extends HostWrite {
  const HostWriteRemoteDecidePairing({
    required this.approve,
    required this.owner,
    required this.pairingId,
  });

  final bool approve;
  final bool owner;
  final String pairingId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'remote.decide_pairing',
      'params': <String, dynamic>{
      'approve': approve,
      'owner': owner,
      'pairing_id': pairingId,
    },
    };
}

final class HostWriteRemoteDisable extends HostWrite {
  const HostWriteRemoteDisable();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'remote.disable',
    };
}

final class HostWriteRemoteEnable extends HostWrite {
  const HostWriteRemoteEnable();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'remote.enable',
    };
}

final class HostWriteRemoteRevoke extends HostWrite {
  const HostWriteRemoteRevoke({
    required this.clientId,
  });

  final String clientId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'remote.revoke',
      'params': <String, dynamic>{
      'client_id': clientId,
    },
    };
}

/// A new one-time code (5 minutes) plus the QR payload; the receipt keeps neither.
final class HostWriteRemoteStartPairing extends HostWrite {
  const HostWriteRemoteStartPairing();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'remote.start_pairing',
    };
}

final class HostWriteSecretSet extends HostWrite {
  const HostWriteSecretSet({
    required this.connectionId,
    required this.secret,
  });

  final String connectionId;
  final String secret;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'secret.set',
      'params': <String, dynamic>{
      'connection_id': connectionId,
      'secret': secret,
    },
    };
}

final class HostWriteServerRestart extends HostWrite {
  const HostWriteServerRestart();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'server.restart',
    };
}

/// End one session (any board member's: one role). Its device's open
/// connection closes at once (`ServerMessage::SignedOut`); the timeline says
/// who revoked it. Answers `Done`. Not for apps.
final class HostWriteSessionRevoke extends HostWrite {
  const HostWriteSessionRevoke({
    required this.sessionId,
  });

  final String sessionId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'session.revoke',
      'params': <String, dynamic>{
      'session_id': sessionId,
    },
    };
}

/// Trace mode on or off for every run that starts afterwards (feature `run_trace`).
/// Off also forgets the traces kept so far.
final class HostWriteTraceSetEnabled extends HostWrite {
  const HostWriteTraceSetEnabled({
    required this.enabled,
  });

  final bool enabled;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'trace.set_enabled',
      'params': <String, dynamic>{
      'enabled': enabled,
    },
    };
}

final class HostWriteUpdateApply extends HostWrite {
  const HostWriteUpdateApply();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'update.apply',
    };
}

final class HostWriteUpdateApplyApp extends HostWrite {
  const HostWriteUpdateApplyApp();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'update.apply_app',
    };
}

final class HostWriteUpdatePrepare extends HostWrite {
  const HostWriteUpdatePrepare();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'update.prepare',
    };
}

final class HostWriteUpdatePrepareApp extends HostWrite {
  const HostWriteUpdatePrepareApp();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'update.prepare_app',
    };
}

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
final class HostWriteUserCreate extends HostWrite {
  const HostWriteUserCreate({
    required this.params,
  });

  final UserCreateParams params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'user.create',
      'params': params.toJson(),
    };
}

/// Disable a board member: their sessions end at once and they cannot sign
/// in; their Claude login is disconnected; tickets keep their creator. The
/// last active board member cannot be disabled (`last_board_member`).
/// Answers `User`. Not for apps.
final class HostWriteUserDisable extends HostWrite {
  const HostWriteUserDisable({
    required this.userId,
  });

  final String userId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'user.disable',
      'params': <String, dynamic>{
      'user_id': userId,
    },
    };
}

/// Change a password. Own (no `user_id`, or the session's): with the old
/// password; the other sessions end, this one stays. Another board member's
/// (B10): a temporary password, changed at their next sign-in
/// (`must_change_password`); all their sessions end. Like `user.create`, the
/// passwords are in no receipt, request hash, event or log. Answers `Done`.
final class HostWriteUserSetPassword extends HostWrite {
  const HostWriteUserSetPassword({
    required this.params,
  });

  final SetPasswordParams params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'user.set_password',
      'params': params.toJson(),
    };
}

/// Archive ("Delete") an item: runs and employees no longer get it; its audit
/// history stays. Answers `VaultItem`. Not for apps.
final class HostWriteVaultArchive extends HostWrite {
  const HostWriteVaultArchive({
    required this.itemId,
  });

  final String itemId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'vault.archive',
      'params': <String, dynamic>{
      'item_id': itemId,
    },
    };
}

/// A new vault item (feature `vault`), in the company (no `project_id`) or a
/// project. Answers `VaultItem` (`value` only for text/url). The server
/// encrypts the value before anything is recorded: the receipt, its request
/// hash and the event `vault.item_created` hold no value (MU4-05 AC2).
/// `validation` for a name or env name used in the scope, an env name that
/// is not UPPER_SNAKE or that the runtime reserves (`PATH`, `ANTHROPIC_*`,
/// …), a url without `https://`/`http://`, a secret under 8 characters.
/// Board members with a session only (employees and agent tools never); not for apps.
final class HostWriteVaultCreate extends HostWrite {
  const HostWriteVaultCreate({
    required this.params,
  });

  final VaultCreateParams params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'vault.create',
      'params': params.toJson(),
    };
}

/// Change an item; what is left out stays. `value`: a new value (a secret's
/// is never sent back to the editor, so "leave empty to keep"). `kind` only
/// between text and url: to or from secret is `vault_kind_locked` (B7).
/// With `expected_version` in the envelope a stale edit is `version_conflict`.
/// Answers `VaultItem`. As `vault.create`: no value in receipt, hash or event.
final class HostWriteVaultUpdate extends HostWrite {
  const HostWriteVaultUpdate({
    required this.params,
  });

  final VaultUpdateParams params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'vault.update',
      'params': params.toJson(),
    };
}

/// What an idea is about: something new or better, or something broken.
enum IdeaKind {
  bug('bug'),
  feature('feature');

  const IdeaKind(this.wire);

  final String wire;

  static IdeaKind fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown IdeaKind value: $value'),
      );

  String toJson() => wire;
}

enum IdeaStatus {
  /// Done: it became a task, or the user marked it done.
  done('done'),
  /// Not doing it.
  dropped('dropped'),
  /// Still an idea.
  open('open');

  const IdeaStatus(this.wire);

  final String wire;

  static IdeaStatus fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown IdeaStatus value: $value'),
      );

  String toJson() => wire;
}

/// One idea: noted by the user or an employee; it may have become a task.
final class IdeaView {
  const IdeaView({
    this.attachments = const [],
    required this.author,
    required this.createdAtMs,
    this.employeeId,
    required this.id,
    required this.kind,
    this.note,
    required this.projectId,
    required this.status,
    this.taskId,
    this.taskStatus,
    this.ticketId,
    this.ticketNumber,
    required this.title,
    required this.updatedAtMs,
  });

  /// Files attached to the idea, each with an `id` for `upload.get` (feature
  /// `idea_attachments`); empty, and left out on older servers.
  final List<MessageFileView> attachments;
  /// "You", or the employee who noted it.
  final String author;
  final int createdAtMs;
  /// The employee who noted it; None when the user did.
  final String? employeeId;
  final String id;
  /// Feature or bug;
  final IdeaKind kind;
  final String? note;
  final String projectId;
  final IdeaStatus status;
  /// The task it became, and that task's title and status.
  final String? taskId;
  final TaskStatus? taskStatus;
  /// The ticket made from it and that ticket's number (feature `ticket_from_idea`);
  /// left out when there is none.
  final String? ticketId;
  final int? ticketNumber;
  final String title;
  final int updatedAtMs;

  factory IdeaView.fromJson(Map<String, dynamic> json) => IdeaView(
    attachments: (json['attachments'] == null ? const [] : (json['attachments'] as List<dynamic>).map((x0) => MessageFileView.fromJson(x0 as Map<String, dynamic>)).toList()),
    author: (json['author'] as String),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    id: (json['id'] as String),
    kind: IdeaKind.fromJson(json['kind'] as String),
    note: (json['note'] == null ? null : (json['note'] as String)),
    projectId: (json['project_id'] as String),
    status: IdeaStatus.fromJson(json['status'] as String),
    taskId: (json['task_id'] == null ? null : (json['task_id'] as String)),
    taskStatus: (json['task_status'] == null ? null : TaskStatus.fromJson(json['task_status'] as String)),
    ticketId: (json['ticket_id'] == null ? null : (json['ticket_id'] as String)),
    ticketNumber: (json['ticket_number'] == null ? null : (json['ticket_number'] as num).toInt()),
    title: (json['title'] as String),
    updatedAtMs: (json['updated_at_ms'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'attachments': attachments.map((x0) => x0.toJson()).toList(),
    'author': author,
    'created_at_ms': createdAtMs,
    if (employeeId != null) 'employee_id': employeeId,
    'id': id,
    'kind': kind.toJson(),
    if (note != null) 'note': note,
    'project_id': projectId,
    'status': status.toJson(),
    if (taskId != null) 'task_id': taskId,
    if (taskStatus != null) 'task_status': taskStatus?.toJson(),
    if (ticketId != null) 'ticket_id': ticketId,
    if (ticketNumber != null) 'ticket_number': ticketNumber,
    'title': title,
    'updated_at_ms': updatedAtMs,
  };
}

/// A project's ideas.
final class IdeasSnapshot {
  const IdeasSnapshot({
    required this.ideas,
    required this.projectId,
  });

  final List<IdeaView> ideas;
  final String projectId;

  factory IdeasSnapshot.fromJson(Map<String, dynamic> json) => IdeasSnapshot(
    ideas: (json['ideas'] as List<dynamic>).map((x0) => IdeaView.fromJson(x0 as Map<String, dynamic>)).toList(),
    projectId: (json['project_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'ideas': ideas.map((x0) => x0.toJson()).toList(),
    'project_id': projectId,
  };
}

enum JobState {
  cancelled('cancelled'),
  dead('dead'),
  done('done'),
  leased('leased'),
  ready('ready');

  const JobState(this.wire);

  final String wire;

  static JobState fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown JobState value: $value'),
      );

  String toJson() => wire;
}

final class JobSummary {
  const JobSummary({
    this.conversationId,
    required this.createdAtMs,
    required this.id,
    required this.kind,
    required this.label,
    this.live,
    this.notBeforeMs,
    required this.progressPermille,
    this.reason,
    required this.state,
    this.taskId,
  });

  /// What it works on: the conversation of an answer, or the task.
  final String? conversationId;
  final int createdAtMs;
  final String id;
  final String kind;
  final String label;
  /// While it runs: since when and the tokens so far.
  final RunProgress? live;
  /// A ready job that waits: until when (none when it waits for the user
  /// without a time, e.g. "auth"), and why ("quota", "auth", "transient", ...).
  final int? notBeforeMs;
  final int progressPermille;
  final String? reason;
  final JobState state;
  final String? taskId;

  factory JobSummary.fromJson(Map<String, dynamic> json) => JobSummary(
    conversationId: (json['conversation_id'] == null ? null : (json['conversation_id'] as String)),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    id: (json['id'] as String),
    kind: (json['kind'] as String),
    label: (json['label'] as String),
    live: (json['live'] == null ? null : RunProgress.fromJson(json['live'] as Map<String, dynamic>)),
    notBeforeMs: (json['not_before_ms'] == null ? null : (json['not_before_ms'] as num).toInt()),
    progressPermille: (json['progress_permille'] as num).toInt(),
    reason: (json['reason'] == null ? null : (json['reason'] as String)),
    state: JobState.fromJson(json['state'] as String),
    taskId: (json['task_id'] == null ? null : (json['task_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (conversationId != null) 'conversation_id': conversationId,
    'created_at_ms': createdAtMs,
    'id': id,
    'kind': kind,
    'label': label,
    if (live != null) 'live': live?.toJson(),
    if (notBeforeMs != null) 'not_before_ms': notBeforeMs,
    'progress_permille': progressPermille,
    if (reason != null) 'reason': reason,
    'state': state.toJson(),
    if (taskId != null) 'task_id': taskId,
  };
}

final class JobsSnapshot {
  const JobsSnapshot({
    required this.asOfSeq,
    required this.jobs,
  });

  final int asOfSeq;
  final List<JobSummary> jobs;

  factory JobsSnapshot.fromJson(Map<String, dynamic> json) => JobsSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    jobs: (json['jobs'] as List<dynamic>).map((x0) => JobSummary.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'jobs': jobs.map((x0) => x0.toJson()).toList(),
  };
}

/// `vault.key_export`. The password is never printed.
final class KeyExportParams {
  const KeyExportParams({
    required this.password,
  });

  /// The signed-in board member's own password, asked again.
  final String password;

  factory KeyExportParams.fromJson(Map<String, dynamic> json) => KeyExportParams(
    password: (json['password'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'password': password,
  };
}

final class LessonEvidence {
  const LessonEvidence({
    required this.label,
    required this.text,
  });

  /// S1..S8 as the extraction saw it.
  final String label;
  final String text;

  factory LessonEvidence.fromJson(Map<String, dynamic> json) => LessonEvidence(
    label: (json['label'] as String),
    text: (json['text'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'label': label,
    'text': text,
  };
}

/// A lesson's place in `locator_json.lesson`: when it applies, its evidence
/// (label and signal text), how often later tasks confirmed it and from which tasks.
final class LessonLocator {
  const LessonLocator({
    required this.appliesWhen,
    this.evidence = const [],
    this.reinforced = 0,
    this.tasks = const [],
  });

  final String appliesWhen;
  final List<LessonEvidence> evidence;
  /// How often a later task found the same lesson again.
  final int reinforced;
  final List<String> tasks;

  factory LessonLocator.fromJson(Map<String, dynamic> json) => LessonLocator(
    appliesWhen: (json['applies_when'] as String),
    evidence: (json['evidence'] == null ? const [] : (json['evidence'] as List<dynamic>).map((x0) => LessonEvidence.fromJson(x0 as Map<String, dynamic>)).toList()),
    reinforced: (json['reinforced'] == null ? 0 : (json['reinforced'] as num).toInt()),
    tasks: (json['tasks'] == null ? const [] : (json['tasks'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'applies_when': appliesWhen,
    'evidence': evidence.map((x0) => x0.toJson()).toList(),
    'reinforced': reinforced,
    'tasks': tasks,
  };
}

enum LifecycleStatus {
  active('active'),
  archived('archived'),
  paused('paused');

  const LifecycleStatus(this.wire);

  final String wire;

  static LifecycleStatus fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown LifecycleStatus value: $value'),
      );

  String toJson() => wire;
}

/// What a finished task's follow-up run does (`TaskView::live_run`).
enum LiveRun {
  /// Its run waits in the queue and has not been claimed yet.
  queued('queued'),
  /// A run is going (created, starting, running, waiting or cancelling).
  running('running');

  const LiveRun(this.wire);

  final String wire;

  static LiveRun fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown LiveRun value: $value'),
      );

  String toJson() => wire;
}

enum LoadGate {
  held('held'),
  open('open');

  const LoadGate(this.wire);

  final String wire;

  static LoadGate fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown LoadGate value: $value'),
      );

  String toJson() => wire;
}

final class LoadHold {
  const LoadHold({
    this.cpuPct,
    required this.kind,
    this.memFreeMb,
    this.running,
  });

  final double? cpuPct;
  final LoadHoldKind kind;
  final int? memFreeMb;
  final int? running;

  factory LoadHold.fromJson(Map<String, dynamic> json) => LoadHold(
    cpuPct: (json['cpu_pct'] == null ? null : (json['cpu_pct'] as num).toDouble()),
    kind: LoadHoldKind.fromJson(json['kind'] as String),
    memFreeMb: (json['mem_free_mb'] == null ? null : (json['mem_free_mb'] as num).toInt()),
    running: (json['running'] == null ? null : (json['running'] as num).toInt()),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (cpuPct != null) 'cpu_pct': cpuPct,
    'kind': kind.toJson(),
    if (memFreeMb != null) 'mem_free_mb': memFreeMb,
    if (running != null) 'running': running,
  };
}

enum LoadHoldKind {
  cpu('cpu'),
  /// `max_runs` is 0: every new run waits.
  disabledAll('disabled_all'),
  maxRuns('max_runs'),
  memory('memory');

  const LoadHoldKind(this.wire);

  final String wire;

  static LoadHoldKind fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown LoadHoldKind value: $value'),
      );

  String toJson() => wire;
}

/// When a new run may start.
final class LoadLimits {
  const LoadLimits({
    required this.cpuHigh,
    required this.cpuLow,
    required this.enabled,
    required this.maxRuns,
    required this.memMinMb,
  });

  /// Hold new runs from this CPU use (%)…
  final int cpuHigh;
  /// …until it is below this again.
  final int cpuLow;
  /// Whether CPU and memory count (the run limit always does).
  final bool enabled;
  /// Runs at the same time; 0 holds every new run.
  final int maxRuns;
  final int memMinMb;

  factory LoadLimits.fromJson(Map<String, dynamic> json) => LoadLimits(
    cpuHigh: (json['cpu_high'] as num).toInt(),
    cpuLow: (json['cpu_low'] as num).toInt(),
    enabled: (json['enabled'] as bool),
    maxRuns: (json['max_runs'] as num).toInt(),
    memMinMb: (json['mem_min_mb'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'cpu_high': cpuHigh,
    'cpu_low': cpuLow,
    'enabled': enabled,
    'max_runs': maxRuns,
    'mem_min_mb': memMinMb,
  };
}

/// `load/set_limits`: `None` leaves a limit as it is.
final class LoadLimitsUpdate {
  const LoadLimitsUpdate({
    this.cpuHigh,
    this.cpuLow,
    this.enabled,
    this.maxRuns,
    this.memMinMb,
  });

  final int? cpuHigh;
  final int? cpuLow;
  final bool? enabled;
  final int? maxRuns;
  final int? memMinMb;

  factory LoadLimitsUpdate.fromJson(Map<String, dynamic> json) => LoadLimitsUpdate(
    cpuHigh: (json['cpu_high'] == null ? null : (json['cpu_high'] as num).toInt()),
    cpuLow: (json['cpu_low'] == null ? null : (json['cpu_low'] as num).toInt()),
    enabled: (json['enabled'] == null ? null : (json['enabled'] as bool)),
    maxRuns: (json['max_runs'] == null ? null : (json['max_runs'] as num).toInt()),
    memMinMb: (json['mem_min_mb'] == null ? null : (json['mem_min_mb'] as num).toInt()),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (cpuHigh != null) 'cpu_high': cpuHigh,
    if (cpuLow != null) 'cpu_low': cpuLow,
    if (enabled != null) 'enabled': enabled,
    if (maxRuns != null) 'max_runs': maxRuns,
    if (memMinMb != null) 'mem_min_mb': memMinMb,
  };
}

/// The machine's load and the run admission (`load.get`, `load_changed`).
final class LoadSnapshot {
  const LoadSnapshot({
    required this.checksActive,
    required this.cpuPct,
    required this.gate,
    required this.held,
    this.hold,
    required this.limits,
    required this.memFreeMb,
    required this.memTotalMb,
    required this.running,
    required this.sampledAtMs,
  });

  /// Whether CPU and memory are measured at all (only for the standard data
  /// folder; elsewhere only `max_runs` counts).
  final bool checksActive;
  /// All cores together, 0–100, averaged over ~10 s.
  final double cpuPct;
  final LoadGate gate;
  /// Runs that could start but wait for the machine.
  final int held;
  /// Why new runs wait (`gate` is `held`).
  final LoadHold? hold;
  final LoadLimits limits;
  final int memFreeMb;
  final int memTotalMb;
  /// Runs going now (chat answers and task starts).
  final int running;
  final int sampledAtMs;

  factory LoadSnapshot.fromJson(Map<String, dynamic> json) => LoadSnapshot(
    checksActive: (json['checks_active'] as bool),
    cpuPct: (json['cpu_pct'] as num).toDouble(),
    gate: LoadGate.fromJson(json['gate'] as String),
    held: (json['held'] as num).toInt(),
    hold: (json['hold'] == null ? null : LoadHold.fromJson(json['hold'] as Map<String, dynamic>)),
    limits: LoadLimits.fromJson(json['limits'] as Map<String, dynamic>),
    memFreeMb: (json['mem_free_mb'] as num).toInt(),
    memTotalMb: (json['mem_total_mb'] as num).toInt(),
    running: (json['running'] as num).toInt(),
    sampledAtMs: (json['sampled_at_ms'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'checks_active': checksActive,
    'cpu_pct': cpuPct,
    'gate': gate.toJson(),
    'held': held,
    if (hold != null) 'hold': hold?.toJson(),
    'limits': limits.toJson(),
    'mem_free_mb': memFreeMb,
    'mem_total_mb': memTotalMb,
    'running': running,
    'sampled_at_ms': sampledAtMs,
  };
}

/// `user.login`. The password is never printed.
final class LoginParams {
  const LoginParams({
    required this.deviceName,
    required this.password,
    required this.username,
  });

  /// How the device shows in `session.list` ("Desktop · dev-laptop").
  final String deviceName;
  final String password;
  final String username;

  factory LoginParams.fromJson(Map<String, dynamic> json) => LoginParams(
    deviceName: (json['device_name'] as String),
    password: (json['password'] as String),
    username: (json['username'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'device_name': deviceName,
    'password': password,
    'username': username,
  };
}

/// `user.login`: what the client keeps for the server. The token is shown
/// here once; the server keeps only its SHA-256. Never printed.
final class LoginResult {
  const LoginResult({
    required this.expiresAtMs,
    required this.mustChangePassword,
    required this.sessionToken,
    required this.user,
  });

  /// When the session ends at the latest (90 days); 30 days without use end it earlier.
  final int expiresAtMs;
  /// A temporary password: change it with `user.set_password` first.
  final bool mustChangePassword;
  /// Give it in every hello (`HelloParams::session_token`) on this device.
  final String sessionToken;
  final UserView user;

  factory LoginResult.fromJson(Map<String, dynamic> json) => LoginResult(
    expiresAtMs: (json['expires_at_ms'] as num).toInt(),
    mustChangePassword: (json['must_change_password'] as bool),
    sessionToken: (json['session_token'] as String),
    user: UserView.fromJson(json['user'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'expires_at_ms': expiresAtMs,
    'must_change_password': mustChangePassword,
    'session_token': sessionToken,
    'user': user.toJson(),
  };
}

/// `management_mode.get_setting` and `management_mode.set_enabled` (feature
/// `management_chooses_mode`): whether management (the CEO and their direct reports) chooses
/// its own task mode, Auto or Plan, when the user picked none. On by default.
final class ManagementModeSetting {
  const ManagementModeSetting({
    required this.enabled,
  });

  final bool enabled;

  factory ManagementModeSetting.fromJson(Map<String, dynamic> json) => ManagementModeSetting(
    enabled: (json['enabled'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'enabled': enabled,
  };
}

final class MarkdownFileContent {
  const MarkdownFileContent({
    required this.byteLength,
    required this.modifiedMs,
    required this.path,
    required this.text,
  });

  final int byteLength;
  final int modifiedMs;
  /// Relative to the work folder, with `/` (also when asked by absolute path).
  final String path;
  final String text;

  factory MarkdownFileContent.fromJson(Map<String, dynamic> json) => MarkdownFileContent(
    byteLength: (json['byte_length'] as num).toInt(),
    modifiedMs: (json['modified_ms'] as num).toInt(),
    path: (json['path'] as String),
    text: (json['text'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'byte_length': byteLength,
    'modified_ms': modifiedMs,
    'path': path,
    'text': text,
  };
}

final class MarkdownFileEntry {
  const MarkdownFileEntry({
    required this.byteLength,
    required this.modifiedMs,
    required this.path,
  });

  final int byteLength;
  final int modifiedMs;
  /// Relative to the work folder, with `/`.
  final String path;

  factory MarkdownFileEntry.fromJson(Map<String, dynamic> json) => MarkdownFileEntry(
    byteLength: (json['byte_length'] as num).toInt(),
    modifiedMs: (json['modified_ms'] as num).toInt(),
    path: (json['path'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'byte_length': byteLength,
    'modified_ms': modifiedMs,
    'path': path,
  };
}

final class MarkdownFileList {
  const MarkdownFileList({
    required this.files,
    required this.root,
    required this.truncated,
  });

  /// Sorted by path.
  final List<MarkdownFileEntry> files;
  /// The work folder that was listed (empty when there is none on the server).
  final String root;
  /// More files than `MARKDOWN_MAX_FILES` exist; the rest is left out.
  final bool truncated;

  factory MarkdownFileList.fromJson(Map<String, dynamic> json) => MarkdownFileList(
    files: (json['files'] as List<dynamic>).map((x0) => MarkdownFileEntry.fromJson(x0 as Map<String, dynamic>)).toList(),
    root: (json['root'] as String),
    truncated: (json['truncated'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'files': files.map((x0) => x0.toJson()).toList(),
    'root': root,
    'truncated': truncated,
  };
}

/// What `mcp/add` sends. Env and header values are secrets: `Debug`
/// never prints them and the server never returns them.
final class McpAddParams {
  const McpAddParams({
    this.args = const [],
    this.command,
    this.enable = false,
    this.env = const {},
    this.headers = const {},
    required this.name,
    required this.transport,
    this.url,
  });

  final List<String> args;
  /// Stdio: the program to run.
  final String? command;
  /// Also switch it on for every run.
  final bool enable;
  final Map<String, String> env;
  final Map<String, String> headers;
  final String name;
  final McpTransport transport;
  /// Http and sse: the endpoint.
  final String? url;

  factory McpAddParams.fromJson(Map<String, dynamic> json) => McpAddParams(
    args: (json['args'] == null ? const [] : (json['args'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    command: (json['command'] == null ? null : (json['command'] as String)),
    enable: (json['enable'] == null ? false : (json['enable'] as bool)),
    env: (json['env'] == null ? const {} : (json['env'] as Map<String, dynamic>).map((k0, v0) => MapEntry(k0, (v0 as String)))),
    headers: (json['headers'] == null ? const {} : (json['headers'] as Map<String, dynamic>).map((k0, v0) => MapEntry(k0, (v0 as String)))),
    name: (json['name'] as String),
    transport: McpTransport.fromJson(json['transport'] as String),
    url: (json['url'] == null ? null : (json['url'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'args': args,
    if (command != null) 'command': command,
    'enable': enable,
    'env': env,
    'headers': headers,
    'name': name,
    'transport': transport.toJson(),
    if (url != null) 'url': url,
  };
}

/// `mcp_always_allow.list` and `.remove`: the MCP tools (`mcp__<server>__<tool>`)
/// the user answered with 'Always allow'. They pass without asking in every run, in
/// Ask, Accept edits and Auto; Plan mode still asks.
final class McpAlwaysAllow {
  const McpAlwaysAllow({
    required this.tools,
  });

  final List<String> tools;

  factory McpAlwaysAllow.fromJson(Map<String, dynamic> json) => McpAlwaysAllow(
    tools: (json['tools'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'tools': tools,
  };
}

/// The OAuth state of an MCP server, read from Claude Code's own credential store.
enum McpAuthStatus {
  /// Claude Code has a usable token (it refreshes it itself).
  authorized('authorized'),
  /// The server asks for a login (`mcp.login_start`).
  needsLogin('needs_login'),
  /// No OAuth: stdio, or a fixed `Authorization` header.
  none('none'),
  /// Not known yet (never connected); a login can still be tried.
  unknown('unknown');

  const McpAuthStatus(this.wire);

  final String wire;

  static McpAuthStatus fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown McpAuthStatus value: $value'),
      );

  String toJson() => wire;
}

/// `mcp.login_start`, `mcp.login_complete` and `mcp.login_get`. Never holds a token.
final class McpLogin {
  const McpLogin({
    required this.authorizationUrl,
    this.error,
    required this.expiresAtMs,
    required this.loginId,
    required this.name,
    required this.redirectUri,
    required this.state,
  });

  /// Open this in a browser. When the browser runs on the server's computer the
  /// login finishes by itself; elsewhere, the browser ends on a page that does
  /// not load: send that page's address to `mcp.login_complete`.
  final String authorizationUrl;
  /// Why it failed.
  final String? error;
  final int expiresAtMs;
  final String loginId;
  /// The MCP server.
  final String name;
  /// The `http://localhost:<port>/callback` address the browser is sent back to.
  final String redirectUri;
  final McpLoginState state;

  factory McpLogin.fromJson(Map<String, dynamic> json) => McpLogin(
    authorizationUrl: (json['authorization_url'] as String),
    error: (json['error'] == null ? null : (json['error'] as String)),
    expiresAtMs: (json['expires_at_ms'] as num).toInt(),
    loginId: (json['login_id'] as String),
    name: (json['name'] as String),
    redirectUri: (json['redirect_uri'] as String),
    state: McpLoginState.fromJson(json['state'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'authorization_url': authorizationUrl,
    if (error != null) 'error': error,
    'expires_at_ms': expiresAtMs,
    'login_id': loginId,
    'name': name,
    'redirect_uri': redirectUri,
    'state': state.toJson(),
  };
}

/// Where a login started with `mcp.login_start` stands.
enum McpLoginState {
  /// Not finished in time; start again.
  expired('expired'),
  failed('failed'),
  /// Waiting for the browser (the server's own callback) or for `mcp.login_complete`.
  pending('pending'),
  succeeded('succeeded');

  const McpLoginState(this.wire);

  final String wire;

  static McpLoginState fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown McpLoginState value: $value'),
      );

  String toJson() => wire;
}

/// Where Claude Code keeps an MCP server the user configured.
enum McpScope {
  /// `~/.claude.json`, for all projects.
  user('user');

  const McpScope(this.wire);

  final String wire;

  static McpScope fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown McpScope value: $value'),
      );

  String toJson() => wire;
}

/// One MCP server of the user. Never holds env values, headers or tokens.
final class McpServerInfo {
  const McpServerInfo({
    this.auth,
    required this.enabled,
    required this.name,
    required this.scope,
    required this.summary,
    required this.transport,
  });

  /// Whether it needs an OAuth login (http/sse); older servers leave it out (`unknown`).
  final McpAuthStatus? auth;
  /// Every run uses it.
  final bool enabled;
  /// The name the user gave it; it is the key for `mcp.set`.
  final String name;
  final McpScope scope;
  /// The command (without arguments) or the URL (without credentials or query).
  final String summary;
  final McpTransport transport;

  factory McpServerInfo.fromJson(Map<String, dynamic> json) => McpServerInfo(
    auth: (json['auth'] == null ? null : McpAuthStatus.fromJson(json['auth'] as String)),
    enabled: (json['enabled'] as bool),
    name: (json['name'] as String),
    scope: McpScope.fromJson(json['scope'] as String),
    summary: (json['summary'] as String),
    transport: McpTransport.fromJson(json['transport'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (auth != null) 'auth': auth?.toJson(),
    'enabled': enabled,
    'name': name,
    'scope': scope.toJson(),
    'summary': summary,
    'transport': transport.toJson(),
  };
}

/// `mcp.list` and the MCP writes.
final class McpServers {
  const McpServers({
    required this.missing,
    required this.servers,
  });

  /// Enabled names that Claude Code no longer has; they are skipped in runs.
  final List<String> missing;
  final List<McpServerInfo> servers;

  factory McpServers.fromJson(Map<String, dynamic> json) => McpServers(
    missing: (json['missing'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    servers: (json['servers'] as List<dynamic>).map((x0) => McpServerInfo.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'missing': missing,
    'servers': servers.map((x0) => x0.toJson()).toList(),
  };
}

enum McpTransport {
  http('http'),
  sse('sse'),
  stdio('stdio');

  const McpTransport(this.wire);

  final String wire;

  static McpTransport fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown McpTransport value: $value'),
      );

  String toJson() => wire;
}

final class MemoryContextItem {
  const MemoryContextItem({
    required this.content,
    this.employeeName,
    required this.kind,
    required this.label,
    required this.memoryId,
    required this.pinned,
    required this.placement,
    required this.scope,
    required this.tokens,
  });

  final String content;
  final String? employeeName;
  final MemoryKind kind;
  /// The label the context builder writes, e.g. "Decision · project".
  final String label;
  final String memoryId;
  final bool pinned;
  /// always | when_relevant | over_budget | duplicate
  final String placement;
  /// global | project | employee
  final String scope;
  /// Tokens of the line as it goes into the system text.
  final int tokens;

  factory MemoryContextItem.fromJson(Map<String, dynamic> json) => MemoryContextItem(
    content: (json['content'] as String),
    employeeName: (json['employee_name'] == null ? null : (json['employee_name'] as String)),
    kind: MemoryKind.fromJson(json['kind'] as String),
    label: (json['label'] as String),
    memoryId: (json['memory_id'] as String),
    pinned: (json['pinned'] as bool),
    placement: (json['placement'] as String),
    scope: (json['scope'] as String),
    tokens: (json['tokens'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'content': content,
    if (employeeName != null) 'employee_name': employeeName,
    'kind': kind.toJson(),
    'label': label,
    'memory_id': memoryId,
    'pinned': pinned,
    'placement': placement,
    'scope': scope,
    'tokens': tokens,
  };
}

/// What goes into an employee's context without a specific question.
final class MemoryContextView {
  const MemoryContextView({
    required this.alwaysTokens,
    this.employeeId,
    required this.items,
    required this.memoryBudgetTokens,
    this.projectId,
    required this.systemTokens,
  });

  final int alwaysTokens;
  final String? employeeId;
  final List<MemoryContextItem> items;
  final int memoryBudgetTokens;
  /// None: an employee's chat outside projects.
  final String? projectId;
  /// Estimate of the fixed system text without memory; 0 when not known.
  final int systemTokens;

  factory MemoryContextView.fromJson(Map<String, dynamic> json) => MemoryContextView(
    alwaysTokens: (json['always_tokens'] as num).toInt(),
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    items: (json['items'] as List<dynamic>).map((x0) => MemoryContextItem.fromJson(x0 as Map<String, dynamic>)).toList(),
    memoryBudgetTokens: (json['memory_budget_tokens'] as num).toInt(),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
    systemTokens: (json['system_tokens'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'always_tokens': alwaysTokens,
    if (employeeId != null) 'employee_id': employeeId,
    'items': items.map((x0) => x0.toJson()).toList(),
    'memory_budget_tokens': memoryBudgetTokens,
    if (projectId != null) 'project_id': projectId,
    'system_tokens': systemTokens,
  };
}

enum MemoryKind {
  conversationSummary('conversation_summary'),
  decision('decision'),
  episodic('episodic'),
  identity('identity'),
  projectKnowledge('project_knowledge');

  const MemoryKind(this.wire);

  final String wire;

  static MemoryKind fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown MemoryKind value: $value'),
      );

  String toJson() => wire;
}

/// Where a memory lives. Global only holds what was explicitly made global.
sealed class MemoryScope {
  const MemoryScope();

  factory MemoryScope.fromJson(Map<String, dynamic> json) {
    final tag = json['kind'] as String;
    switch (tag) {
      case 'employee':
        return MemoryScopeEmployee(
          id: (json['id'] as String),
        );
      case 'global':
        return const MemoryScopeGlobal();
      case 'project':
        return MemoryScopeProject(
          id: (json['id'] as String),
        );
      default:
        throw FormatException('Unknown MemoryScope kind: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

final class MemoryScopeEmployee extends MemoryScope {
  const MemoryScopeEmployee({
    required this.id,
  });

  final String id;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'employee',
      'id': id,
    };
}

final class MemoryScopeGlobal extends MemoryScope {
  const MemoryScopeGlobal();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'global',
    };
}

final class MemoryScopeProject extends MemoryScope {
  const MemoryScopeProject({
    required this.id,
  });

  final String id;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'project',
      'id': id,
    };
}

final class MemorySnapshot {
  const MemorySnapshot({
    required this.items,
  });

  final List<MemoryView> items;

  factory MemorySnapshot.fromJson(Map<String, dynamic> json) => MemorySnapshot(
    items: (json['items'] as List<dynamic>).map((x0) => MemoryView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'items': items.map((x0) => x0.toJson()).toList(),
  };
}

final class MemorySourceView {
  const MemorySourceView({
    required this.atMs,
    required this.eventId,
    required this.label,
  });

  final int atMs;
  final String eventId;
  final String label;

  factory MemorySourceView.fromJson(Map<String, dynamic> json) => MemorySourceView(
    atMs: (json['at_ms'] as num).toInt(),
    eventId: (json['event_id'] as String),
    label: (json['label'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'at_ms': atMs,
    'event_id': eventId,
    'label': label,
  };
}

enum MemoryStatus {
  active('active'),
  forgotten('forgotten'),
  proposed('proposed'),
  rejected('rejected'),
  superseded('superseded');

  const MemoryStatus(this.wire);

  final String wire;

  static MemoryStatus fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown MemoryStatus value: $value'),
      );

  String toJson() => wire;
}

final class MemoryView {
  const MemoryView({
    required this.confidence,
    this.conflictsWithId,
    required this.content,
    required this.createdAtMs,
    this.decidedBy,
    this.employeeId,
    this.employeeName,
    required this.id,
    required this.importance,
    required this.kind,
    this.lesson,
    required this.pinned,
    required this.scope,
    required this.sources,
    required this.status,
    this.supersedesId,
  });

  final double confidence;
  final String? conflictsWithId;
  /// Empty when forgotten (content erased).
  final String content;
  final int createdAtMs;
  /// user | policy | None (still proposed)
  final String? decidedBy;
  final String? employeeId;
  final String? employeeName;
  final String id;
  final double importance;
  final MemoryKind kind;
  /// Set when this memory is a lesson learned from a task (feature
  /// `task_lessons`): when it applies, its evidence and how often it was
  /// confirmed. The kind is then `episodic`; show it as "Lesson".
  final LessonLocator? lesson;
  final bool pinned;
  /// global | project | employee
  final String scope;
  final List<MemorySourceView> sources;
  final MemoryStatus status;
  final String? supersedesId;

  factory MemoryView.fromJson(Map<String, dynamic> json) => MemoryView(
    confidence: (json['confidence'] as num).toDouble(),
    conflictsWithId: (json['conflicts_with_id'] == null ? null : (json['conflicts_with_id'] as String)),
    content: (json['content'] as String),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    decidedBy: (json['decided_by'] == null ? null : (json['decided_by'] as String)),
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    employeeName: (json['employee_name'] == null ? null : (json['employee_name'] as String)),
    id: (json['id'] as String),
    importance: (json['importance'] as num).toDouble(),
    kind: MemoryKind.fromJson(json['kind'] as String),
    lesson: (json['lesson'] == null ? null : LessonLocator.fromJson(json['lesson'] as Map<String, dynamic>)),
    pinned: (json['pinned'] as bool),
    scope: (json['scope'] as String),
    sources: (json['sources'] as List<dynamic>).map((x0) => MemorySourceView.fromJson(x0 as Map<String, dynamic>)).toList(),
    status: MemoryStatus.fromJson(json['status'] as String),
    supersedesId: (json['supersedes_id'] == null ? null : (json['supersedes_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'confidence': confidence,
    if (conflictsWithId != null) 'conflicts_with_id': conflictsWithId,
    'content': content,
    'created_at_ms': createdAtMs,
    if (decidedBy != null) 'decided_by': decidedBy,
    if (employeeId != null) 'employee_id': employeeId,
    if (employeeName != null) 'employee_name': employeeName,
    'id': id,
    'importance': importance,
    'kind': kind.toJson(),
    if (lesson != null) 'lesson': lesson?.toJson(),
    'pinned': pinned,
    'scope': scope,
    'sources': sources.map((x0) => x0.toJson()).toList(),
    'status': status.toJson(),
    if (supersedesId != null) 'supersedes_id': supersedesId,
  };
}

/// A file the user sent to an employee, copied into its work folder.
final class MessageFileView {
  const MessageFileView({
    required this.byteLength,
    this.id,
    required this.name,
    required this.path,
  });

  final int byteLength;
  /// Fetch it with `upload.get`. Given by the server when it reads the file's
  /// message, task or recurring task (never stored); older servers leave it out.
  final String? id;
  final String name;
  /// Relative to the work folder, with `/`, e.g. `.aurelius/uploads/report.pdf`.
  final String path;

  factory MessageFileView.fromJson(Map<String, dynamic> json) => MessageFileView(
    byteLength: (json['byte_length'] as num).toInt(),
    id: (json['id'] == null ? null : (json['id'] as String)),
    name: (json['name'] as String),
    path: (json['path'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'byte_length': byteLength,
    if (id != null) 'id': id,
    'name': name,
    'path': path,
  };
}

/// `message.history` (feature `message_edits`): the earlier texts of a message,
/// newest first, each through the vault redactor when it goes out.
final class MessageHistoryView {
  const MessageHistoryView({
    required this.deleted,
    required this.versions,
  });

  final bool deleted;
  /// Always empty when `deleted`: a deleted text is gone in every client, even
  /// though the audit log keeps it.
  final List<MessageVersionView> versions;

  factory MessageHistoryView.fromJson(Map<String, dynamic> json) => MessageHistoryView(
    deleted: (json['deleted'] as bool),
    versions: (json['versions'] as List<dynamic>).map((x0) => MessageVersionView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'deleted': deleted,
    'versions': versions.map((x0) => x0.toJson()).toList(),
  };
}

/// `message.locate` (feature `message_locate`): where a message of a channel or
/// a ticket's discussion is, so a link to it can open the right place.
final class MessageLocation {
  const MessageLocation({
    required this.conversationId,
    required this.messageId,
    this.threadRoot,
    this.ticketId,
    this.ticketNumber,
  });

  final String conversationId;
  final String messageId;
  /// The message that starts the thread it replies in; none for a message of
  /// the channel itself (also for a thread's root).
  final String? threadRoot;
  /// The ticket whose discussion it is in; none for a channel.
  final String? ticketId;
  /// The number of `ticket_id` (`#12`).
  final int? ticketNumber;

  factory MessageLocation.fromJson(Map<String, dynamic> json) => MessageLocation(
    conversationId: (json['conversation_id'] as String),
    messageId: (json['message_id'] as String),
    threadRoot: (json['thread_root'] == null ? null : (json['thread_root'] as String)),
    ticketId: (json['ticket_id'] == null ? null : (json['ticket_id'] as String)),
    ticketNumber: (json['ticket_number'] == null ? null : (json['ticket_number'] as num).toInt()),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversation_id': conversationId,
    'message_id': messageId,
    if (threadRoot != null) 'thread_root': threadRoot,
    if (ticketId != null) 'ticket_id': ticketId,
    if (ticketNumber != null) 'ticket_number': ticketNumber,
  };
}

enum MessageRole {
  assistant('assistant'),
  user('user');

  const MessageRole(this.wire);

  final String wire;

  static MessageRole fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown MessageRole value: $value'),
      );

  String toJson() => wire;
}

enum MessageStatus {
  complete('complete'),
  failed('failed'),
  interrupted('interrupted'),
  partial('partial');

  const MessageStatus(this.wire);

  final String wire;

  static MessageStatus fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown MessageStatus value: $value'),
      );

  String toJson() => wire;
}

/// One earlier text of an edited message (`MessageHistoryView::versions`).
final class MessageVersionView {
  const MessageVersionView({
    required this.replacedAtMs,
    required this.text,
  });

  /// When an edit replaced this text.
  final int replacedAtMs;
  final String text;

  factory MessageVersionView.fromJson(Map<String, dynamic> json) => MessageVersionView(
    replacedAtMs: (json['replaced_at_ms'] as num).toInt(),
    text: (json['text'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'replaced_at_ms': replacedAtMs,
    'text': text,
  };
}

final class MessageView {
  const MessageView({
    required this.activity,
    this.approvalId,
    this.author,
    this.authorUser,
    this.awaitingUser,
    required this.createdAtMs,
    this.decision,
    this.deletedAtMs,
    this.editable,
    this.editedAtMs,
    required this.files,
    required this.id,
    this.mode,
    required this.ordinal,
    this.producedBy,
    required this.queued,
    this.reactions = const [],
    this.replyTo,
    required this.role,
    this.run,
    this.runId,
    required this.sources,
    required this.status,
    this.system,
    required this.text,
    this.threadLastReplyMs,
    this.threadParent,
    this.threadRepliers = const [],
    this.threadReplyCount,
    this.threadUnread,
    this.update,
    required this.uploads,
    this.usage,
  });

  /// Tools the runtime used while answering (commands, edits, web searches).
  final List<ActivityView> activity;
  /// For an `approval` update: the approval it asks for (decided here or in the inbox).
  final String? approvalId;
  /// Persona that answered (e.g. "Marcus"), if any.
  final String? author;
  /// The board member who wrote a user message in a discussion (a ticket's,
  /// a channel's; feature `ticket_discussion`). Left out for other messages
  /// and by older servers.
  final UserView? authorUser;
  /// The run behind this message waits for a board member (feature `run_cards`):
  /// a question or an approval is open, or the post ended a ticket's main task on
  /// a question (the rule of `group_chat::asks_question`). Only on the run's last
  /// message in the list. Left out when false and by older servers.
  final bool? awaitingUser;
  final int createdAtMs;
  /// A board member's answer to a run's questions or decision on its plan (feature
  /// `run_decisions`), on the `status` update that notes it, so a client can show it
  /// as a card without reading `text` apart. Left out for other messages, for
  /// decisions noted before this field existed and by older servers.
  final RunDecision? decision;
  /// When the message was deleted (feature `message_edits`): `text` is the
  /// line that says so. Left out when it was not and by older servers.
  final int? deletedAtMs;
  /// The reading board member may edit and delete this message (feature
  /// `message_edits`), per reader like `ReactionView::mine`. Left out when false.
  final bool? editable;
  /// When the message was last edited (feature `message_edits`). Left out when
  /// it never was and by older servers.
  final int? editedAtMs;
  /// Files the message mentions, kept with it (open them with `query/file`).
  final List<FileRef> files;
  final String id;
  /// An employee's answer in the group chat: the mode its run started in.
  /// Left out for other messages and by older servers.
  final AgentMode? mode;
  final int ordinal;
  /// "Connection · model" that produced an assistant message.
  final String? producedBy;
  /// A user message whose answer has not started yet: it waits behind the
  /// answer running in its conversation. Such a message can be deleted
  /// (`DeleteQueuedMessage`) or sent now (`SendQueuedNow`). Left out when false.
  final bool queued;
  /// The reactions on the message (feature `chat_reactions`), one entry per
  /// emoji, in the order each emoji was first used. Left out when there are
  /// none and by older servers.
  final List<ReactionView> reactions;
  /// For retried answers: the user message this answers again.
  final String? replyTo;
  final MessageRole role;
  /// What a run card under this message shows (feature `run_cards`): on a post in a
  /// discussion and on a `Reported` line, the last one of its run in the list. With
  /// `usage` for the tokens. Left out for other messages and by older servers.
  final RunBrief? run;
  /// Run that produced an assistant message.
  final String? runId;
  /// Web sources fetched for this answer, cited as [n].
  final List<SourceView> sources;
  final MessageStatus status;
  /// A system line of Aurelius in a discussion (feature `ticket_discussion`):
  /// what happened, shown as a compact grey line; `text` is its readable form.
  /// Such a line never wakes anyone. Left out for other messages and by older servers.
  final SystemLine? system;
  final String text;
  /// A message that starts a thread (feature `chat_threads`): when its latest reply was written.
  final int? threadLastReplyMs;
  /// A reply in a thread (feature `chat_threads`): the message of the channel
  /// it answers (the thread's root, never itself a reply). Replies are only in
  /// `thread.read`, never in the channel's own list. Left out for a message of
  /// the channel itself and by older servers.
  final String? threadParent;
  /// A message that starts a thread (feature `chat_threads`): who replied, so
  /// a client can show them under the root. The names (board members'
  /// usernames, employees' names, as in `ReactionView::by`) of at most
  /// `THREAD_REPLIERS_MAX` (3) distinct repliers, most recent first. Left out when
  /// it has no replies and by older servers.
  final List<String> threadRepliers;
  /// A message that starts a thread (feature `chat_threads`): how many replies it has.
  /// Left out when it has none and by older servers.
  final int? threadReplyCount;
  /// A message that starts a thread (feature `chat_threads`): how many of its
  /// replies the reading board member has not read yet (others' replies since
  /// their last `thread.read` of it). Reading the channel does not change it.
  /// Left out when none are unread, for an app, and by older servers.
  final int? threadUnread;
  /// A task update in the task's conversation, not an answer: brief |
  /// finding | question | status. `author` says from whom (a
  /// subagent, a colleague).
  final String? update;
  /// Files the user uploaded with this message into the work folder.
  final List<MessageFileView> uploads;
  /// The tokens the answer's run used, once its runtime reported them (feature
  /// `run_usage`). Left out for user messages and by older servers.
  final TokenUsage? usage;

  factory MessageView.fromJson(Map<String, dynamic> json) => MessageView(
    activity: (json['activity'] as List<dynamic>).map((x0) => ActivityView.fromJson(x0 as Map<String, dynamic>)).toList(),
    approvalId: (json['approval_id'] == null ? null : (json['approval_id'] as String)),
    author: (json['author'] == null ? null : (json['author'] as String)),
    authorUser: (json['author_user'] == null ? null : UserView.fromJson(json['author_user'] as Map<String, dynamic>)),
    awaitingUser: (json['awaiting_user'] == null ? null : (json['awaiting_user'] as bool)),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    decision: (json['decision'] == null ? null : RunDecision.fromJson(json['decision'] as Map<String, dynamic>)),
    deletedAtMs: (json['deleted_at_ms'] == null ? null : (json['deleted_at_ms'] as num).toInt()),
    editable: (json['editable'] == null ? null : (json['editable'] as bool)),
    editedAtMs: (json['edited_at_ms'] == null ? null : (json['edited_at_ms'] as num).toInt()),
    files: (json['files'] as List<dynamic>).map((x0) => FileRef.fromJson(x0 as Map<String, dynamic>)).toList(),
    id: (json['id'] as String),
    mode: (json['mode'] == null ? null : AgentMode.fromJson(json['mode'] as String)),
    ordinal: (json['ordinal'] as num).toInt(),
    producedBy: (json['produced_by'] == null ? null : (json['produced_by'] as String)),
    queued: (json['queued'] as bool),
    reactions: (json['reactions'] == null ? const [] : (json['reactions'] as List<dynamic>).map((x0) => ReactionView.fromJson(x0 as Map<String, dynamic>)).toList()),
    replyTo: (json['reply_to'] == null ? null : (json['reply_to'] as String)),
    role: MessageRole.fromJson(json['role'] as String),
    run: (json['run'] == null ? null : RunBrief.fromJson(json['run'] as Map<String, dynamic>)),
    runId: (json['run_id'] == null ? null : (json['run_id'] as String)),
    sources: (json['sources'] as List<dynamic>).map((x0) => SourceView.fromJson(x0 as Map<String, dynamic>)).toList(),
    status: MessageStatus.fromJson(json['status'] as String),
    system: (json['system'] == null ? null : SystemLine.fromJson(json['system'] as Map<String, dynamic>)),
    text: (json['text'] as String),
    threadLastReplyMs: (json['thread_last_reply_ms'] == null ? null : (json['thread_last_reply_ms'] as num).toInt()),
    threadParent: (json['thread_parent'] == null ? null : (json['thread_parent'] as String)),
    threadRepliers: (json['thread_repliers'] == null ? const [] : (json['thread_repliers'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    threadReplyCount: (json['thread_reply_count'] == null ? null : (json['thread_reply_count'] as num).toInt()),
    threadUnread: (json['thread_unread'] == null ? null : (json['thread_unread'] as num).toInt()),
    update: (json['update'] == null ? null : (json['update'] as String)),
    uploads: (json['uploads'] as List<dynamic>).map((x0) => MessageFileView.fromJson(x0 as Map<String, dynamic>)).toList(),
    usage: (json['usage'] == null ? null : TokenUsage.fromJson(json['usage'] as Map<String, dynamic>)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'activity': activity.map((x0) => x0.toJson()).toList(),
    if (approvalId != null) 'approval_id': approvalId,
    if (author != null) 'author': author,
    if (authorUser != null) 'author_user': authorUser?.toJson(),
    if (awaitingUser != null) 'awaiting_user': awaitingUser,
    'created_at_ms': createdAtMs,
    if (decision != null) 'decision': decision?.toJson(),
    if (deletedAtMs != null) 'deleted_at_ms': deletedAtMs,
    if (editable != null) 'editable': editable,
    if (editedAtMs != null) 'edited_at_ms': editedAtMs,
    'files': files.map((x0) => x0.toJson()).toList(),
    'id': id,
    if (mode != null) 'mode': mode?.toJson(),
    'ordinal': ordinal,
    if (producedBy != null) 'produced_by': producedBy,
    'queued': queued,
    'reactions': reactions.map((x0) => x0.toJson()).toList(),
    if (replyTo != null) 'reply_to': replyTo,
    'role': role.toJson(),
    if (run != null) 'run': run?.toJson(),
    if (runId != null) 'run_id': runId,
    'sources': sources.map((x0) => x0.toJson()).toList(),
    'status': status.toJson(),
    if (system != null) 'system': system?.toJson(),
    'text': text,
    if (threadLastReplyMs != null) 'thread_last_reply_ms': threadLastReplyMs,
    if (threadParent != null) 'thread_parent': threadParent,
    'thread_repliers': threadRepliers,
    if (threadReplyCount != null) 'thread_reply_count': threadReplyCount,
    if (threadUnread != null) 'thread_unread': threadUnread,
    if (update != null) 'update': update,
    'uploads': uploads.map((x0) => x0.toJson()).toList(),
    if (usage != null) 'usage': usage?.toJson(),
  };
}

final class MessagesSnapshot {
  const MessagesSnapshot({
    required this.agentMode,
    required this.asOfSeq,
    required this.attachments,
    required this.browserAllowed,
    required this.conversationId,
    this.employeeId,
    required this.messages,
    this.projectId,
    this.readOnly,
    required this.run,
    required this.screenAllowed,
  });

  final AgentMode agentMode;
  final int asOfSeq;
  final List<AttachmentView> attachments;
  final bool browserAllowed;
  final String conversationId;
  /// The employee of a task conversation or an employee chat.
  final String? employeeId;
  final List<MessageView> messages;
  /// The project of a task or project conversation; none for an employee chat.
  final String? projectId;
  /// An older employee chat (converted from protocol 2): history, it takes no messages.
  final bool? readOnly;
  /// Model and effort chosen for this conversation (unset: the employee's).
  final TaskRun run;
  /// The agent may take screenshots without asking each time; off by default (it then asks).
  final bool screenAllowed;

  factory MessagesSnapshot.fromJson(Map<String, dynamic> json) => MessagesSnapshot(
    agentMode: AgentMode.fromJson(json['agent_mode'] as String),
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    attachments: (json['attachments'] as List<dynamic>).map((x0) => AttachmentView.fromJson(x0 as Map<String, dynamic>)).toList(),
    browserAllowed: (json['browser_allowed'] as bool),
    conversationId: (json['conversation_id'] as String),
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    messages: (json['messages'] as List<dynamic>).map((x0) => MessageView.fromJson(x0 as Map<String, dynamic>)).toList(),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
    readOnly: (json['read_only'] == null ? null : (json['read_only'] as bool)),
    run: TaskRun.fromJson(json['run'] as Map<String, dynamic>),
    screenAllowed: (json['screen_allowed'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'agent_mode': agentMode.toJson(),
    'as_of_seq': asOfSeq,
    'attachments': attachments.map((x0) => x0.toJson()).toList(),
    'browser_allowed': browserAllowed,
    'conversation_id': conversationId,
    if (employeeId != null) 'employee_id': employeeId,
    'messages': messages.map((x0) => x0.toJson()).toList(),
    if (projectId != null) 'project_id': projectId,
    if (readOnly != null) 'read_only': readOnly,
    'run': run.toJson(),
    'screen_allowed': screenAllowed,
  };
}

sealed class Method {
  const Method();

  factory Method.fromJson(Map<String, dynamic> json) {
    final tag = json['method'] as String;
    switch (tag) {
      case 'activity.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodActivityGet(
          callId: (src['call_id'] as String),
          messageId: (src['message_id'] as String),
        );
        }
      case 'activity.overview':
        return const MethodActivityOverview();
      case 'activity_log.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodActivityLogList(
          beforeSeq: (src['before_seq'] == null ? null : (src['before_seq'] as num).toInt()),
          category: (src['category'] == null ? null : (src['category'] as String)),
          employeeId: (src['employee_id'] == null ? null : (src['employee_id'] as String)),
          limit: (src['limit'] == null ? null : (src['limit'] as num).toInt()),
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
          runId: (src['run_id'] == null ? null : (src['run_id'] as String)),
          threadKey: (src['thread_key'] == null ? null : (src['thread_key'] as String)),
          ticketNumber: (src['ticket_number'] == null ? null : (src['ticket_number'] as num).toInt()),
        );
        }
      case 'approval.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodApprovalList(
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      case 'auto_allow.get_setting':
        return const MethodAutoAllowGetSetting();
      case 'backup.list':
        return const MethodBackupList();
      case 'board.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodBoardGet(
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      case 'browser.get':
        return const MethodBrowserGet();
      case 'channel.list':
        return const MethodChannelList();
      case 'channel.search':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodChannelSearch(
          query: (src['query'] as String),
        );
        }
      case 'claude_login.input':
        return MethodClaudeLoginInput(params: ClaudeLoginInputParams.fromJson(json['params'] as Map<String, dynamic>));
      case 'claude_login.status':
        return const MethodClaudeLoginStatus();
      case 'code.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodCodeGet(
          projectId: (src['project_id'] as String),
        );
        }
      case 'command.applied':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodCommandApplied(
          idempotencyKey: (src['idempotency_key'] as String),
        );
        }
      case 'compaction.get_setting':
        return const MethodCompactionGetSetting();
      case 'company.get':
        return const MethodCompanyGet();
      case 'connection.detect_runtime':
        return const MethodConnectionDetectRuntime();
      case 'connection.list':
        return const MethodConnectionList();
      case 'conversation.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodConversationGet(
          conversationId: (src['conversation_id'] as String),
        );
        }
      case 'discussion.audience':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodDiscussionAudience(
          channel: (src['channel'] == null ? null : (src['channel'] as String)),
        );
        }
      case 'employee.chat':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodEmployeeChat(
          employeeId: (src['employee_id'] as String),
        );
        }
      case 'employee.chats':
        return const MethodEmployeeChats();
      case 'employee.instruction_versions':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodEmployeeInstructionVersions(
          employeeId: (src['employee_id'] as String),
        );
        }
      case 'employee.list':
        return const MethodEmployeeList();
      case 'employee.queue':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodEmployeeQueue(
          employeeId: (src['employee_id'] as String),
        );
        }
      case 'employee.run_overview':
        return const MethodEmployeeRunOverview();
      case 'employee.scratchpad':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodEmployeeScratchpad(
          employeeId: (src['employee_id'] as String),
        );
        }
      case 'employee.start_profiles':
        return const MethodEmployeeStartProfiles();
      case 'employee.tool_options':
        return const MethodEmployeeToolOptions();
      case 'event.subscribe':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodEventSubscribe(
          afterSeq: (src['after_seq'] as num).toInt(),
        );
        }
      case 'file.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodFileGet(
          id: (src['id'] as String),
        );
        }
      case 'fs.list_dir':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodFsListDir(
          path: (src['path'] == null ? null : (src['path'] as String)),
        );
        }
      case 'group_chat.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodGroupChatGet(
          channel: (src['channel'] == null ? null : (src['channel'] as String)),
        );
        }
      case 'guideline.affected':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodGuidelineAffected(
          guidelineId: (src['guideline_id'] as String),
        );
        }
      case 'guideline.effective':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodGuidelineEffective(
          employeeId: (src['employee_id'] == null ? null : (src['employee_id'] as String)),
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      case 'guideline.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodGuidelineGet(
          guidelineId: (src['guideline_id'] as String),
          version: (src['version'] == null ? null : (src['version'] as num).toInt()),
        );
        }
      case 'guideline.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodGuidelineList(
          archived: (src['archived'] == null ? null : (src['archived'] as bool)),
        );
        }
      case 'guideline.proposals':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodGuidelineProposals(
          status: (src['status'] == null ? null : ProposalStatus.fromJson(src['status'] as String)),
        );
        }
      case 'guideline.starter_set':
        return const MethodGuidelineStarterSet();
      case 'guideline.versions':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodGuidelineVersions(
          guidelineId: (src['guideline_id'] as String),
        );
        }
      case 'idea.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodIdeaList(
          projectId: (src['project_id'] as String),
        );
        }
      case 'idea.list_all':
        return const MethodIdeaListAll();
      case 'job.list':
        return const MethodJobList();
      case 'load.get':
        return const MethodLoadGet();
      case 'management_mode.get_setting':
        return const MethodManagementModeGetSetting();
      case 'markdown.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodMarkdownGet(
          path: (src['path'] as String),
          scope: FileScope.fromJson(src['scope'] as Map<String, dynamic>),
        );
        }
      case 'markdown.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodMarkdownList(
          scope: FileScope.fromJson(src['scope'] as Map<String, dynamic>),
        );
        }
      case 'mcp.list':
        return const MethodMcpList();
      case 'mcp.login_get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodMcpLoginGet(
          loginId: (src['login_id'] as String),
        );
        }
      case 'mcp_always_allow.list':
        return const MethodMcpAlwaysAllowList();
      case 'memory.get_context':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodMemoryGetContext(
          employeeId: (src['employee_id'] == null ? null : (src['employee_id'] as String)),
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      case 'memory.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodMemoryList(
          employeeId: (src['employee_id'] == null ? null : (src['employee_id'] as String)),
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      case 'message.history':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodMessageHistory(
          messageId: (src['message_id'] as String),
        );
        }
      case 'message.locate':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodMessageLocate(
          messageId: (src['message_id'] as String),
        );
        }
      case 'notification.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodNotificationList(
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      case 'power.get':
        return const MethodPowerGet();
      case 'project.list':
        return const MethodProjectList();
      case 'project_conversation.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodProjectConversationList(
          projectId: (src['project_id'] as String),
        );
        }
      case 'proposal.list':
        return const MethodProposalList();
      case 'recurring_task.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodRecurringTaskList(
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      case 'remote.get_status':
        return const MethodRemoteGetStatus();
      case 'run.get_activity':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodRunGetActivity(
          runId: (src['run_id'] as String),
        );
        }
      case 'run.get_context':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodRunGetContext(
          runId: (src['run_id'] as String),
        );
        }
      case 'run.get_trace':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodRunGetTrace(
          runId: (src['run_id'] as String),
        );
        }
      case 'run.guidelines':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodRunGuidelines(
          runId: (src['run_id'] as String),
        );
        }
      case 'scheduled_message.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodScheduledMessageList(
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      case 'screenshot.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodScreenshotGet(
          id: (src['id'] as String),
        );
        }
      case 'server.get_health':
        return const MethodServerGetHealth();
      case 'server.get_name':
        return const MethodServerGetName();
      case 'session.hello':
        return MethodSessionHello(params: HelloParams.fromJson(json['params'] as Map<String, dynamic>));
      case 'session.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodSessionList(
          userId: (src['user_id'] == null ? null : (src['user_id'] as String)),
        );
        }
      case 'subagent.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodSubagentList(
          conversationId: (src['conversation_id'] as String),
        );
        }
      case 'subscription.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodSubscriptionGet(
          refresh: (src['refresh'] as bool),
        );
        }
      case 'task.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodTaskList(
          employeeId: (src['employee_id'] == null ? null : (src['employee_id'] as String)),
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
          state: (src['state'] == null ? null : TaskListState.fromJson(src['state'] as String)),
        );
        }
      case 'thread.read':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodThreadRead(
          messageId: (src['message_id'] as String),
        );
        }
      case 'thread.waiting':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodThreadWaiting(
          conversationId: (src['conversation_id'] == null ? null : (src['conversation_id'] as String)),
          ticketId: (src['ticket_id'] == null ? null : (src['ticket_id'] as String)),
        );
        }
      case 'ticket.followers':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodTicketFollowers(
          ticketId: (src['ticket_id'] as String),
        );
        }
      case 'ticket.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodTicketGet(
          ticketId: (src['ticket_id'] as String),
        );
        }
      case 'ticket.history':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodTicketHistory(
          beforeDoneAtMs: (src['before_done_at_ms'] == null ? null : (src['before_done_at_ms'] as num).toInt()),
          beforeNumber: (src['before_number'] == null ? null : (src['before_number'] as num).toInt()),
          employeeId: (src['employee_id'] == null ? null : (src['employee_id'] as String)),
          fromMs: (src['from_ms'] == null ? null : (src['from_ms'] as num).toInt()),
          limit: (src['limit'] == null ? null : (src['limit'] as num).toInt()),
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
          query: (src['query'] == null ? null : (src['query'] as String)),
          toMs: (src['to_ms'] == null ? null : (src['to_ms'] as num).toInt()),
        );
        }
      case 'ticket.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodTicketList(
          column: (src['column'] == null ? null : ColumnKind.fromJson(src['column'] as String)),
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
          query: (src['query'] == null ? null : (src['query'] as String)),
        );
        }
      case 'ticket.my_tickets':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodTicketMyTickets(
          includeDone: (src['include_done'] == null ? null : (src['include_done'] as bool)),
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      case 'timeline.list':
        return MethodTimelineList(params: TimelineQuery.fromJson(json['params'] as Map<String, dynamic>));
      case 'trace.get_setting':
        return const MethodTraceGetSetting();
      case 'update.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodUpdateGet(
          check: (src['check'] as bool),
        );
        }
      case 'upload.get':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodUploadGet(
          id: (src['id'] as String),
          maxSide: (src['max_side'] == null ? null : (src['max_side'] as num).toInt()),
          offset: (src['offset'] == null ? null : (src['offset'] as num).toInt()),
        );
        }
      case 'usage.get':
        return const MethodUsageGet();
      case 'usage.get_hours':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodUsageGetHours(
          dayStartMs: (src['day_start_ms'] as num).toInt(),
        );
        }
      case 'usage.get_totals':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodUsageGetTotals(
          conversationId: (src['conversation_id'] == null ? null : (src['conversation_id'] as String)),
          taskId: (src['task_id'] == null ? null : (src['task_id'] as String)),
          ticketId: (src['ticket_id'] == null ? null : (src['ticket_id'] as String)),
        );
        }
      case 'user.list':
        return const MethodUserList();
      case 'user.login':
        return MethodUserLogin(params: LoginParams.fromJson(json['params'] as Map<String, dynamic>));
      case 'user.logout':
        return const MethodUserLogout();
      case 'user.whoami':
        return const MethodUserWhoami();
      case 'vault.audit':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodVaultAudit(
          all: (src['all'] == null ? null : (src['all'] as bool)),
          before: (src['before'] == null ? null : (src['before'] as num).toInt()),
          itemId: (src['item_id'] == null ? null : (src['item_id'] as String)),
          limit: (src['limit'] == null ? null : (src['limit'] as num).toInt()),
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      case 'vault.key_export':
        return MethodVaultKeyExport(params: KeyExportParams.fromJson(json['params'] as Map<String, dynamic>));
      case 'vault.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodVaultList(
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      case 'vault.reveal':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodVaultReveal(
          itemId: (src['item_id'] as String),
          purpose: RevealPurpose.fromJson(src['purpose'] as String),
        );
        }
      case 'work_session.history':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodWorkSessionHistory(
          beforeConversationId: (src['before_conversation_id'] == null ? null : (src['before_conversation_id'] as String)),
          beforeLastActivityMs: (src['before_last_activity_ms'] == null ? null : (src['before_last_activity_ms'] as num).toInt()),
          employeeId: (src['employee_id'] == null ? null : (src['employee_id'] as String)),
          fromMs: (src['from_ms'] == null ? null : (src['from_ms'] as num).toInt()),
          limit: (src['limit'] == null ? null : (src['limit'] as num).toInt()),
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
          query: (src['query'] == null ? null : (src['query'] as String)),
          toMs: (src['to_ms'] == null ? null : (src['to_ms'] as num).toInt()),
        );
        }
      case 'work_session.list':
        {
          final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return MethodWorkSessionList(
          projectId: (src['project_id'] == null ? null : (src['project_id'] as String)),
        );
        }
      default:
        throw FormatException('Unknown Method method: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

/// One tool call of an answer whole: the input and output that
/// `conversation.get` and live updates shorten (feature `activity_preview`).
final class MethodActivityGet extends Method {
  const MethodActivityGet({
    required this.callId,
    required this.messageId,
  });

  final String callId;
  final String messageId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'activity.get',
      'params': <String, dynamic>{
      'call_id': callId,
      'message_id': messageId,
    },
    };
}

/// Who is busy and what waits, on what (feature `activity_overview`). Another
/// board member's private chat is only counted, with its employee.
final class MethodActivityOverview extends Method {
  const MethodActivityOverview();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'activity.overview',
    };
}

/// The activity log, newest first (R8, feature `activity_log`): every delivery, queue
/// step and run step, narrowed by what is given. `limit`: at most 200, default 100.
/// `project_id` (feature `workspace_filters`): only threads of that project's tickets,
/// channels and tasks; a board member's private chat has no project.
final class MethodActivityLogList extends Method {
  const MethodActivityLogList({
    this.beforeSeq,
    this.category,
    this.employeeId,
    this.limit,
    this.projectId,
    this.runId,
    this.threadKey,
    this.ticketNumber,
  });

  final int? beforeSeq;
  /// delivery, queue or run.
  final String? category;
  final String? employeeId;
  final int? limit;
  final String? projectId;
  final String? runId;
  /// `ticket:<id>`, `channel:<conversation>` or `chat:<conversation>`.
  final String? threadKey;
  /// A ticket's thread, by its number.
  final int? ticketNumber;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'activity_log.list',
      'params': <String, dynamic>{
      if (beforeSeq != null) 'before_seq': beforeSeq,
      if (category != null) 'category': category,
      if (employeeId != null) 'employee_id': employeeId,
      if (limit != null) 'limit': limit,
      if (projectId != null) 'project_id': projectId,
      if (runId != null) 'run_id': runId,
      if (threadKey != null) 'thread_key': threadKey,
      if (ticketNumber != null) 'ticket_number': ticketNumber,
    },
    };
}

/// Pending approvals, then the latest decisions. `project_id` (feature
/// `workspace_filters`): only those of runs in that project.
final class MethodApprovalList extends Method {
  const MethodApprovalList({
    this.projectId,
  });

  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'approval.list',
      'params': <String, dynamic>{
      if (projectId != null) 'project_id': projectId,
    },
    };
}

/// Whether the safe read-only tools pass without a question (the auto-allow list).
final class MethodAutoAllowGetSetting extends Method {
  const MethodAutoAllowGetSetting();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'auto_allow.get_setting',
    };
}

final class MethodBackupList extends Method {
  const MethodBackupList();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'backup.list',
    };
}

/// The board: its six columns left to right, each with its cards (not
/// archived), for one project or every project.
final class MethodBoardGet extends Method {
  const MethodBoardGet({
    this.projectId,
  });

  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'board.get',
      'params': <String, dynamic>{
      if (projectId != null) 'project_id': projectId,
    },
    };
}

final class MethodBrowserGet extends Method {
  const MethodBrowserGet();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'browser.get',
    };
}

/// The channels of the Team chat (feature `team_channels`): the general
/// one first, then one per project (archived projects too: read-only).
final class MethodChannelList extends Method {
  const MethodChannelList();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'channel.list',
    };
}

/// The channels whose name or ticket number matches `query` (feature
/// `ticket_channels`), in the order of `channel.list`, archived ones too: a
/// closed ticket's channel stays findable. `#12` and `12` match ticket 12;
/// other text matches the name, case not counting. A connection that does not
/// accept `ticket_channels` gets no ticket channels here either.
final class MethodChannelSearch extends Method {
  const MethodChannelSearch({
    required this.query,
  });

  final String query;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'channel.search',
      'params': <String, dynamic>{
      'query': query,
    },
    };
}

/// What a running `claude_login.link` asks for: the code Claude showed after
/// the sign-in at Anthropic, written to `claude auth login` as typed. Only
/// the connection that started the link. A session method: no receipt, no
/// request hash, no event, never printed or logged. Answers `ClaudeLogin`.
final class MethodClaudeLoginInput extends Method {
  const MethodClaudeLoginInput({
    required this.params,
  });

  final ClaudeLoginInputParams params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'claude_login.input',
      'params': params.toJson(),
    };
}

/// The session's own Claude login (`ClaudeLoginView`). Feature of protocol 5.
final class MethodClaudeLoginStatus extends Method {
  const MethodClaudeLoginStatus();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'claude_login.status',
    };
}

/// The Code tab of a workspace (feature `code_view`, ticket #77): the git state of
/// the project's folder (worktrees, local branches, recent commits). A folder that is
/// no git repository answers a view with `problem` set, not an error. The same
/// rights as `board.get`.
final class MethodCodeGet extends Method {
  const MethodCodeGet({
    required this.projectId,
  });

  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'code.get',
      'params': <String, dynamic>{
      'project_id': projectId,
    },
    };
}

/// Whether the server committed the write sent with `idempotency_key` (feature
/// `command_applied`), answered with `CommandApplied`. A client asks this after
/// the connection dropped or timed out under a write it will not resend, so it
/// can tell "it arrived, the answer was lost" from "it never arrived". Only yes
/// or no: the receipt's result stays with the connection that wrote it.
final class MethodCommandApplied extends Method {
  const MethodCommandApplied({
    required this.idempotencyKey,
  });

  final String idempotencyKey;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'command.applied',
      'params': <String, dynamic>{
      'idempotency_key': idempotencyKey,
    },
    };
}

/// Above how many tokens a session is compacted (feature `compaction_setting`).
final class MethodCompactionGetSetting extends Method {
  const MethodCompactionGetSetting();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'compaction.get_setting',
    };
}

/// The company in one read: employees, the org chart setting and the
/// management team (roots).
final class MethodCompanyGet extends Method {
  const MethodCompanyGet();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'company.get',
    };
}

/// Where Claude Code is installed on the server.
final class MethodConnectionDetectRuntime extends Method {
  const MethodConnectionDetectRuntime();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'connection.detect_runtime',
    };
}

final class MethodConnectionList extends Method {
  const MethodConnectionList();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'connection.list',
    };
}

/// A conversation's messages and settings.
final class MethodConversationGet extends Method {
  const MethodConversationGet({
    required this.conversationId,
  });

  final String conversationId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'conversation.get',
      'params': <String, dynamic>{
      'conversation_id': conversationId,
    },
    };
}

/// Whom `@Everyone` would reach in a discussion (feature `everyone_mention`),
/// answered with `DiscussionAudience`, for the reader's own message (the
/// reader is never counted). `channel`: a channel or a ticket's discussion
/// (its conversation id, as in `channel.list`); none: the general channel.
/// Read-only; `@Everyone` itself is plain text in `group_chat.send` / `ticket.post`.
final class MethodDiscussionAudience extends Method {
  const MethodDiscussionAudience({
    this.channel,
  });

  final String? channel;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'discussion.audience',
      'params': <String, dynamic>{
      if (channel != null) 'channel': channel,
    },
    };
}

/// An employee's persistent chat with the user (company scope, no project):
/// its messages and settings, as `conversation.get` answers them.
final class MethodEmployeeChat extends Method {
  const MethodEmployeeChat({
    required this.employeeId,
  });

  final String employeeId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'employee.chat',
      'params': <String, dynamic>{
      'employee_id': employeeId,
    },
    };
}

/// The reader's lasting chats with employees, the latest first (feature `employee_chats`).
final class MethodEmployeeChats extends Method {
  const MethodEmployeeChats();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'employee.chats',
    };
}

/// The versions of an employee's instructions, newest first (feature `instruction_proposals`).
final class MethodEmployeeInstructionVersions extends Method {
  const MethodEmployeeInstructionVersions({
    required this.employeeId,
  });

  final String employeeId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'employee.instruction_versions',
      'params': <String, dynamic>{
      'employee_id': employeeId,
    },
    };
}

/// The company's employees (not archived), oldest first.
final class MethodEmployeeList extends Method {
  const MethodEmployeeList();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'employee.list',
    };
}

/// What is in an employee's mailbox now (feature `employee_queue`).
final class MethodEmployeeQueue extends Method {
  const MethodEmployeeQueue({
    required this.employeeId,
  });

  final String employeeId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'employee.queue',
      'params': <String, dynamic>{
      'employee_id': employeeId,
    },
    };
}

/// The model and effort a new run of each employee uses (feature `employee_run_overview`).
final class MethodEmployeeRunOverview extends Method {
  const MethodEmployeeRunOverview();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'employee.run_overview',
    };
}

/// An employee's scratchpad (feature `employee_scratchpad`).
final class MethodEmployeeScratchpad extends Method {
  const MethodEmployeeScratchpad({
    required this.employeeId,
  });

  final String employeeId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'employee.scratchpad',
      'params': <String, dynamic>{
      'employee_id': employeeId,
    },
    };
}

/// The catalogue of start profiles (feature `employee_profiles`).
final class MethodEmployeeStartProfiles extends Method {
  const MethodEmployeeStartProfiles();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'employee.start_profiles',
    };
}

/// The tools that can be switched per employee (feature `employee_tools`).
final class MethodEmployeeToolOptions extends Method {
  const MethodEmployeeToolOptions();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'employee.tool_options',
    };
}

/// Replay events after `after_seq`, then stream live events (and load/power pushes).
final class MethodEventSubscribe extends Method {
  const MethodEventSubscribe({
    required this.afterSeq,
  });

  final int afterSeq;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'event.subscribe',
      'params': <String, dynamic>{
      'after_seq': afterSeq,
    },
    };
}

/// A file kept with a message (see `MessageView::files`).
final class MethodFileGet extends Method {
  const MethodFileGet({
    required this.id,
  });

  final String id;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'file.get',
      'params': <String, dynamic>{
      'id': id,
    },
    };
}

/// Folders on the server: none lists the starting points.
final class MethodFsListDir extends Method {
  const MethodFsListDir({
    this.path,
  });

  final String? path;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'fs.list_dir',
      'params': <String, dynamic>{
      if (path != null) 'path': path,
    },
    };
}

/// The company's group chat (Team chat): the user and every employee. Its
/// messages and settings, as `conversation.get` answers them; an employee's
/// messages carry its name as `author`. Write with `group_chat.send`.
/// `channel` (feature `team_channels`): a project's channel from
/// `channel.list`; none: the general channel, the one older servers have.
final class MethodGroupChatGet extends Method {
  const MethodGroupChatGet({
    this.channel,
  });

  final String? channel;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'group_chat.get',
      'params': <String, dynamic>{
      if (channel != null) 'channel': channel,
    },
    };
}

/// Who a change of this guideline reaches from their next run (B6): employees,
/// projects and conversations that start a new session.
final class MethodGuidelineAffected extends Method {
  const MethodGuidelineAffected({
    required this.guidelineId,
  });

  final String guidelineId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'guideline.affected',
      'params': <String, dynamic>{
      'guideline_id': guidelineId,
    },
    };
}

/// The set a run of this employee in this project would get now, in block
/// order, with `total_chars` against `limit_chars` (B4). Only an employee:
/// its own chat (employee guidelines); only a project: its guidelines alone.
final class MethodGuidelineEffective extends Method {
  const MethodGuidelineEffective({
    this.employeeId,
    this.projectId,
  });

  final String? employeeId;
  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'guideline.effective',
      'params': <String, dynamic>{
      if (employeeId != null) 'employee_id': employeeId,
      if (projectId != null) 'project_id': projectId,
    },
    };
}

/// One guideline with the text of its current version, or of `version`.
final class MethodGuidelineGet extends Method {
  const MethodGuidelineGet({
    required this.guidelineId,
    this.version,
  });

  final String guidelineId;
  final int? version;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'guideline.get',
      'params': <String, dynamic>{
      'guideline_id': guidelineId,
      if (version != null) 'version': version,
    },
    };
}

/// The guideline library (feature `guidelines`): every guideline without its
/// text, alphabetical; archived ones only with `archived` (then only those).
final class MethodGuidelineList extends Method {
  const MethodGuidelineList({
    this.archived,
  });

  final bool? archived;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'guideline.list',
      'params': <String, dynamic>{
      if (archived != null) 'archived': archived,
    },
    };
}

/// Proposals of `propose_guideline` (B5), pending first, then the latest
/// decisions; `status` filters. Apart from `proposal.list`.
final class MethodGuidelineProposals extends Method {
  const MethodGuidelineProposals({
    this.status,
  });

  final ProposalStatus? status;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'guideline.proposals',
      'params': <String, dynamic>{
      if (status != null) 'status': status?.toJson(),
    },
    };
}

/// The templates of the starter set (B7), for "Add starter set…".
final class MethodGuidelineStarterSet extends Method {
  const MethodGuidelineStarterSet();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'guideline.starter_set',
    };
}

/// Every version of a guideline, newest first, with its text and source.
final class MethodGuidelineVersions extends Method {
  const MethodGuidelineVersions({
    required this.guidelineId,
  });

  final String guidelineId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'guideline.versions',
      'params': <String, dynamic>{
      'guideline_id': guidelineId,
    },
    };
}

final class MethodIdeaList extends Method {
  const MethodIdeaList({
    required this.projectId,
  });

  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'idea.list',
      'params': <String, dynamic>{
      'project_id': projectId,
    },
    };
}

/// The ideas of every project (each keeps its own project).
final class MethodIdeaListAll extends Method {
  const MethodIdeaListAll();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'idea.list_all',
      'params': <String, dynamic>{},
    };
}

final class MethodJobList extends Method {
  const MethodJobList();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'job.list',
    };
}

final class MethodLoadGet extends Method {
  const MethodLoadGet();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'load.get',
    };
}

/// Whether management chooses its own task mode (feature `management_chooses_mode`).
final class MethodManagementModeGetSetting extends Method {
  const MethodManagementModeGetSetting();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'management_mode.get_setting',
    };
}

/// One Markdown file as UTF-8 text. `path` is relative to the work folder.
final class MethodMarkdownGet extends Method {
  const MethodMarkdownGet({
    required this.path,
    required this.scope,
  });

  final String path;
  final FileScope scope;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'markdown.get',
      'params': <String, dynamic>{
      'path': path,
      'scope': scope.toJson(),
    },
    };
}

/// The Markdown files in a work folder, for the in-app viewer.
final class MethodMarkdownList extends Method {
  const MethodMarkdownList({
    required this.scope,
  });

  final FileScope scope;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'markdown.list',
      'params': <String, dynamic>{
      'scope': scope.toJson(),
    },
    };
}

/// The user-scope MCP servers of Claude Code and which of them every run uses.
final class MethodMcpList extends Method {
  const MethodMcpList();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'mcp.list',
    };
}

/// A login started with `mcp.login_start`; kept for a while after it ends.
final class MethodMcpLoginGet extends Method {
  const MethodMcpLoginGet({
    required this.loginId,
  });

  final String loginId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'mcp.login_get',
      'params': <String, dynamic>{
      'login_id': loginId,
    },
    };
}

/// The MCP tools the user always allows (answered an approval with 'Always allow').
final class MethodMcpAlwaysAllowList extends Method {
  const MethodMcpAlwaysAllowList();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'mcp_always_allow.list',
    };
}

/// What goes into a context: an employee's (own, the project if given,
/// global) or a project conversation's (project, global; no employee). At
/// least one of the two.
final class MethodMemoryGetContext extends Method {
  const MethodMemoryGetContext({
    this.employeeId,
    this.projectId,
  });

  final String? employeeId;
  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'memory.get_context',
      'params': <String, dynamic>{
      if (employeeId != null) 'employee_id': employeeId,
      if (projectId != null) 'project_id': projectId,
    },
    };
}

/// Memories, newest first: global always, plus the scopes asked for (a
/// project, an employee). Nothing asked: global and every employee.
final class MethodMemoryList extends Method {
  const MethodMemoryList({
    this.employeeId,
    this.projectId,
  });

  final String? employeeId;
  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'memory.list',
      'params': <String, dynamic>{
      if (employeeId != null) 'employee_id': employeeId,
      if (projectId != null) 'project_id': projectId,
    },
    };
}

/// The earlier texts of a message, newest first (feature `message_edits`);
/// none for a deleted one. Not for apps.
final class MethodMessageHistory extends Method {
  const MethodMessageHistory({
    required this.messageId,
  });

  final String messageId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'message.history',
      'params': <String, dynamic>{
      'message_id': messageId,
    },
    };
}

/// Where a message is (feature `message_locate`), for a link to it: its
/// conversation, ticket and thread. Only a message of a channel or a ticket's
/// discussion; any other is `not_found`, as an unknown one. Not for apps.
final class MethodMessageLocate extends Method {
  const MethodMessageLocate({
    required this.messageId,
  });

  final String messageId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'message.locate',
      'params': <String, dynamic>{
      'message_id': messageId,
    },
    };
}

/// The notifications of the board member, newest first. `project_id` (feature
/// `workspace_filters`): only those about that project's tickets, tasks or
/// conversations; `unread` then counts only those too.
final class MethodNotificationList extends Method {
  const MethodNotificationList({
    this.projectId,
  });

  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'notification.list',
      'params': <String, dynamic>{
      if (projectId != null) 'project_id': projectId,
    },
    };
}

final class MethodPowerGet extends Method {
  const MethodPowerGet();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'power.get',
    };
}

final class MethodProjectList extends Method {
  const MethodProjectList();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'project.list',
    };
}

/// The conversations of a project, most recently active first.
final class MethodProjectConversationList extends Method {
  const MethodProjectConversationList({
    required this.projectId,
  });

  final String projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'project_conversation.list',
      'params': <String, dynamic>{
      'project_id': projectId,
    },
    };
}

/// Proposals of the whole company (colleagues and instructions): pending
/// first, then the latest decisions.
final class MethodProposalList extends Method {
  const MethodProposalList();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'proposal.list',
    };
}

final class MethodRecurringTaskList extends Method {
  const MethodRecurringTaskList({
    this.projectId,
  });

  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'recurring_task.list',
      'params': <String, dynamic>{
      if (projectId != null) 'project_id': projectId,
    },
    };
}

final class MethodRemoteGetStatus extends Method {
  const MethodRemoteGetStatus();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'remote.get_status',
    };
}

/// Every tool call of a run, for an opened run card (feature `run_cards`).
final class MethodRunGetActivity extends Method {
  const MethodRunGetActivity({
    required this.runId,
  });

  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'run.get_activity',
      'params': <String, dynamic>{
      'run_id': runId,
    },
    };
}

/// The context manifest of one run (context inspector).
final class MethodRunGetContext extends Method {
  const MethodRunGetContext({
    required this.runId,
  });

  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'run.get_context',
      'params': <String, dynamic>{
      'run_id': runId,
    },
    };
}

/// What one run sent to the runtime and what each model call cost; not found
/// unless trace mode was on for it and it is among the latest (feature `run_trace`).
final class MethodRunGetTrace extends Method {
  const MethodRunGetTrace({
    required this.runId,
  });

  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'run.get_trace',
      'params': <String, dynamic>{
      'run_id': runId,
    },
    };
}

/// The guideline versions a run had in its system text (B6), kept for every
/// run (the trace's "Guidelines: Rust v3, …"); also without trace mode.
final class MethodRunGuidelines extends Method {
  const MethodRunGuidelines({
    required this.runId,
  });

  final String runId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'run.guidelines',
      'params': <String, dynamic>{
      'run_id': runId,
    },
    };
}

/// Messages employees scheduled: what waits, then what went out (feature `scheduled_messages`).
/// `project_id` (feature `workspace_filters`): only those in a ticket or channel of that project.
final class MethodScheduledMessageList extends Method {
  const MethodScheduledMessageList({
    this.projectId,
  });

  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'scheduled_message.list',
      'params': <String, dynamic>{
      if (projectId != null) 'project_id': projectId,
    },
    };
}

/// A screenshot the agent took (the id is in its screenshot tool's output).
final class MethodScreenshotGet extends Method {
  const MethodScreenshotGet({
    required this.id,
  });

  final String id;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'screenshot.get',
      'params': <String, dynamic>{
      'id': id,
    },
    };
}

final class MethodServerGetHealth extends Method {
  const MethodServerGetHealth();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'server.get_health',
    };
}

/// The server's name, its source and the computer's name (feature `server_name`).
final class MethodServerGetName extends Method {
  const MethodServerGetName();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'server.get_name',
    };
}

/// The first request of a session: who is calling. The protocol major in
/// every request must equal the server's, else the answer is a hard error.
final class MethodSessionHello extends Method {
  const MethodSessionHello({
    required this.params,
  });

  final HelloParams params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'session.hello',
      'params': params.toJson(),
    };
}

/// The signed-in devices of a board member (none: the session's own),
/// newest activity first (`SessionView::current` marks this one). Any board
/// member may list another's (one role). Never a token or its hash.
final class MethodSessionList extends Method {
  const MethodSessionList({
    this.userId,
  });

  final String? userId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'session.list',
      'params': <String, dynamic>{
      if (userId != null) 'user_id': userId,
    },
    };
}

/// The subagents running in the answers of one conversation.
final class MethodSubagentList extends Method {
  const MethodSubagentList({
    required this.conversationId,
  });

  final String conversationId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'subagent.list',
      'params': <String, dynamic>{
      'conversation_id': conversationId,
    },
    };
}

final class MethodSubscriptionGet extends Method {
  const MethodSubscriptionGet({
    required this.refresh,
  });

  final bool refresh;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'subscription.get',
      'params': <String, dynamic>{
      'refresh': refresh,
    },
    };
}

/// Tasks, newest first, company-wide; every given filter applies.
final class MethodTaskList extends Method {
  const MethodTaskList({
    this.employeeId,
    this.projectId,
    this.state,
  });

  final String? employeeId;
  final String? projectId;
  final TaskListState? state;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'task.list',
      'params': <String, dynamic>{
      if (employeeId != null) 'employee_id': employeeId,
      if (projectId != null) 'project_id': projectId,
      if (state != null) 'state': state?.toJson(),
    },
    };
}

/// One thread of a channel (feature `chat_threads`): `message_id` is the
/// message that starts it (a message of a channel or a ticket's discussion
/// that is not itself a reply), answered with the message and its replies,
/// oldest first (`ThreadSnapshot`). The board member who reads it has read
/// the thread (its `thread_unread` goes to none); reading the channel never
/// does. A message without replies is a thread of none.
final class MethodThreadRead extends Method {
  const MethodThreadRead({
    required this.messageId,
  });

  final String messageId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'thread.read',
      'params': <String, dynamic>{
      'message_id': messageId,
    },
    };
}

/// Who waits to pick up a thread and what is scheduled in it (U3, feature
/// `scheduled_messages`): a ticket (`ticket_id`) or a channel or chat (`conversation_id`).
final class MethodThreadWaiting extends Method {
  const MethodThreadWaiting({
    this.conversationId,
    this.ticketId,
  });

  final String? conversationId;
  final String? ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'thread.waiting',
      'params': <String, dynamic>{
      if (conversationId != null) 'conversation_id': conversationId,
      if (ticketId != null) 'ticket_id': ticketId,
    },
    };
}

/// Who follows a ticket (feature `ticket_follow`). Not for apps.
final class MethodTicketFollowers extends Method {
  const MethodTicketFollowers({
    required this.ticketId,
  });

  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'ticket.followers',
      'params': <String, dynamic>{
      'ticket_id': ticketId,
    },
    };
}

/// One ticket with its main task and every task linked to it.
final class MethodTicketGet extends Method {
  const MethodTicketGet({
    required this.ticketId,
  });

  final String ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'ticket.get',
      'params': <String, dynamic>{
      'ticket_id': ticketId,
    },
    };
}

/// The History (feature `ticket_history`, ticket #57): tickets on Done, the newest
/// Done first, each with the summary of what it delivered. Every given filter
/// applies: `employee_id` is the ticket's owner, `from_ms`/`to_ms` the period of its
/// Done (`to_ms` excluded), `query` looks in the title and the summary. Keyset
/// paging: the next page passes the last row's `done_at_ms` and `number` as
/// `before_done_at_ms`/`before_number`. `limit`: at most 100, default 50. The same
/// rights as `board.get` (B3); an archived ticket or project stays in it.
final class MethodTicketHistory extends Method {
  const MethodTicketHistory({
    this.beforeDoneAtMs,
    this.beforeNumber,
    this.employeeId,
    this.fromMs,
    this.limit,
    this.projectId,
    this.query,
    this.toMs,
  });

  final int? beforeDoneAtMs;
  final int? beforeNumber;
  final String? employeeId;
  final int? fromMs;
  final int? limit;
  final String? projectId;
  final String? query;
  final int? toMs;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'ticket.history',
      'params': <String, dynamic>{
      if (beforeDoneAtMs != null) 'before_done_at_ms': beforeDoneAtMs,
      if (beforeNumber != null) 'before_number': beforeNumber,
      if (employeeId != null) 'employee_id': employeeId,
      if (fromMs != null) 'from_ms': fromMs,
      if (limit != null) 'limit': limit,
      if (projectId != null) 'project_id': projectId,
      if (query != null) 'query': query,
      if (toMs != null) 'to_ms': toMs,
    },
    };
}

/// Tickets in board order (not archived); every given filter applies. `query`
/// searches title, description and `#number`.
final class MethodTicketList extends Method {
  const MethodTicketList({
    this.column,
    this.projectId,
    this.query,
  });

  final ColumnKind? column;
  final String? projectId;
  final String? query;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'ticket.list',
      'params': <String, dynamic>{
      if (column != null) 'column': column?.toJson(),
      if (projectId != null) 'project_id': projectId,
      if (query != null) 'query': query,
    },
    };
}

/// The tickets of the board member of the session (feature `my_tickets`),
/// never of one named in a param: responsible, created or followed by them,
/// and open ones on their login or without an active responsible. Done and
/// archived ones only with `include_done`. By priority, then the latest
/// update; at most `MY_TICKETS_MAX`. Not for apps.
final class MethodTicketMyTickets extends Method {
  const MethodTicketMyTickets({
    this.includeDone,
    this.projectId,
  });

  final bool? includeDone;
  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'ticket.my_tickets',
      'params': <String, dynamic>{
      if (includeDone != null) 'include_done': includeDone,
      if (projectId != null) 'project_id': projectId,
    },
    };
}

/// Human-readable timeline cards, newest first (keyset on `seq`).
final class MethodTimelineList extends Method {
  const MethodTimelineList({
    required this.params,
  });

  final TimelineQuery params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'timeline.list',
      'params': params.toJson(),
    };
}

/// Whether runs keep a trace (feature `run_trace`).
final class MethodTraceGetSetting extends Method {
  const MethodTraceGetSetting();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'trace.get_setting',
    };
}

/// Update status; `check` asks GitHub for the latest release first.
final class MethodUpdateGet extends Method {
  const MethodUpdateGet({
    required this.check,
  });

  final bool check;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'update.get',
      'params': <String, dynamic>{
      'check': check,
    },
    };
}

/// A file the user uploaded with a message, task, recurring task or idea (see
/// `MessageFileView::id`), in parts of at most `UPLOAD_CHUNK_BYTES` from
/// `offset` (none: 0). With `max_side` an image comes back whole as a JPEG
/// that fits that many pixels (clamped to `UPLOAD_THUMB_MAX_SIDE`); other files are refused.
final class MethodUploadGet extends Method {
  const MethodUploadGet({
    required this.id,
    this.maxSide,
    this.offset,
  });

  final String id;
  final int? maxSide;
  final int? offset;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'upload.get',
      'params': <String, dynamic>{
      'id': id,
      if (maxSide != null) 'max_side': maxSide,
      if (offset != null) 'offset': offset,
    },
    };
}

final class MethodUsageGet extends Method {
  const MethodUsageGet();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'usage.get',
    };
}

/// Usage per hour of one server-local day (any moment in it).
final class MethodUsageGetHours extends Method {
  const MethodUsageGetHours({
    required this.dayStartMs,
  });

  final int dayStartMs;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'usage.get_hours',
      'params': <String, dynamic>{
      'day_start_ms': dayStartMs,
    },
    };
}

/// What a conversation or a task used, with the tasks given away from it apart
/// (feature `run_usage`). Give one of the two; a task wins.
final class MethodUsageGetTotals extends Method {
  const MethodUsageGetTotals({
    this.conversationId,
    this.taskId,
    this.ticketId,
  });

  final String? conversationId;
  final String? taskId;
  /// What a ticket used: its tasks and the runs of its discussion (feature `run_cards`).
  /// Wins over the other two.
  final String? ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'usage.get_totals',
      'params': <String, dynamic>{
      if (conversationId != null) 'conversation_id': conversationId,
      if (taskId != null) 'task_id': taskId,
      if (ticketId != null) 'ticket_id': ticketId,
    },
    };
}

/// The board members, oldest first (never a password's hash), disabled ones
/// too (`UserView::disabled_at_ms`). Works without a session (the sign-in
/// screen may offer the names). Apps may read it too.
final class MethodUserList extends Method {
  const MethodUserList();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'user.list',
    };
}

/// Sign in (protocol 5): a right username and password give this connection a
/// session and answer `Login` with its token, shown once (keep it for the
/// server and give it in every hello). A wrong password, an unknown username
/// and a disabled board member all answer `wrong_password`; after 5 failed tries
/// for one username the server waits longer each time, up to 15 minutes
/// (`too_many_attempts` with `retry_after_ms`). A session method: it changes
/// only the connection and the sessions, so it has no receipt and no event.
/// Not for apps.
final class MethodUserLogin extends Method {
  const MethodUserLogin({
    required this.params,
  });

  final LoginParams params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'user.login',
      'params': params.toJson(),
    };
}

/// End this connection's session (the device must sign in again); answers
/// `WhoAmI` without a board member. A session method.
final class MethodUserLogout extends Method {
  const MethodUserLogout();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'user.logout',
    };
}

/// Whether the server has board members, and this connection's session's one.
/// Works without a session.
final class MethodUserWhoami extends Method {
  const MethodUserWhoami();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'user.whoami',
    };
}

/// The audit log, newest first (`VaultAuditPage`): the company's (nothing
/// given), a project's, or every scope's (`all`); one item's with `item_id`.
/// `before`: rows older than that row id; `limit` at most 200 (default 50).
/// Never a value. Not for apps.
final class MethodVaultAudit extends Method {
  const MethodVaultAudit({
    this.all,
    this.before,
    this.itemId,
    this.limit,
    this.projectId,
  });

  final bool? all;
  final int? before;
  final String? itemId;
  final int? limit;
  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'vault.audit',
      'params': <String, dynamic>{
      if (all != null) 'all': all,
      if (before != null) 'before': before,
      if (itemId != null) 'item_id': itemId,
      if (limit != null) 'limit': limit,
      if (projectId != null) 'project_id': projectId,
    },
    };
}

/// The server's vault key (`VaultKeyExport`, never printed), for a restore on
/// another machine (B8): a board member with a session that needs no new
/// password, who gives their password again (`wrong_password`; it counts
/// for the sign-in limits, `too_many_attempts`). Writes an audit row
/// `key_export` and tells every board member. Not found before the first
/// item. Not for apps.
final class MethodVaultKeyExport extends Method {
  const MethodVaultKeyExport({
    required this.params,
  });

  final KeyExportParams params;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'vault.key_export',
      'params': params.toJson(),
    };
}

/// The vault of a scope (feature `vault`): none, the company's items; a
/// project, its items plus the company's apart (`VaultList::company_items`),
/// with `overrides_company` on a project item whose env name the company
/// has too, and the item count of every scope. Never a secret's value.
/// Archived items are left out. Board members only; not for apps (B5).
final class MethodVaultList extends Method {
  const MethodVaultList({
    this.projectId,
  });

  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'vault.list',
      'params': <String, dynamic>{
      if (projectId != null) 'project_id': projectId,
    },
    };
}

/// One item's value (`VaultValue`, never printed), for a board member with a
/// session. Writes an audit row `reveal` (show) or `copy` with the session's
/// device. `vault_undecryptable` when the key does not fit. Not for apps.
final class MethodVaultReveal extends Method {
  const MethodVaultReveal({
    required this.itemId,
    required this.purpose,
  });

  final String itemId;
  final RevealPurpose purpose;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'vault.reveal',
      'params': <String, dynamic>{
      'item_id': itemId,
      'purpose': purpose.toJson(),
    },
    };
}

/// Work sessions in the History (feature `session_history`): every board member's
/// sessions that were not deleted, the latest activity first, each with the summary
/// of what was done in it. Every given filter applies: `employee_id` is the employee
/// who works in it, `from_ms`/`to_ms` the period of its last activity (`to_ms`
/// excluded), `query` looks in the title and the summary. Keyset paging: the next
/// page passes the last row's `last_activity_ms` and `conversation_id` as
/// `before_last_activity_ms`/`before_conversation_id`. `limit`: at most 100, default
/// 50. The same rights as `ticket.history`.
final class MethodWorkSessionHistory extends Method {
  const MethodWorkSessionHistory({
    this.beforeConversationId,
    this.beforeLastActivityMs,
    this.employeeId,
    this.fromMs,
    this.limit,
    this.projectId,
    this.query,
    this.toMs,
  });

  final String? beforeConversationId;
  final int? beforeLastActivityMs;
  final String? employeeId;
  final int? fromMs;
  final int? limit;
  final String? projectId;
  final String? query;
  final int? toMs;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'work_session.history',
      'params': <String, dynamic>{
      if (beforeConversationId != null) 'before_conversation_id': beforeConversationId,
      if (beforeLastActivityMs != null) 'before_last_activity_ms': beforeLastActivityMs,
      if (employeeId != null) 'employee_id': employeeId,
      if (fromMs != null) 'from_ms': fromMs,
      if (limit != null) 'limit': limit,
      if (projectId != null) 'project_id': projectId,
      if (query != null) 'query': query,
      if (toMs != null) 'to_ms': toMs,
    },
    };
}

/// The reader's work sessions, the latest activity first (feature `work_sessions`);
/// `project_id`: only those of that project.
final class MethodWorkSessionList extends Method {
  const MethodWorkSessionList({
    this.projectId,
  });

  final String? projectId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'method': 'work_session.list',
      'params': <String, dynamic>{
      if (projectId != null) 'project_id': projectId,
    },
    };
}

/// `ticket.my_tickets` (feature `my_tickets`): the tickets of the board member
/// of the session, at most `MY_TICKETS_MAX`. The client makes its tabs and
/// counters from this one answer.
final class MyTicketsView {
  const MyTicketsView({
    required this.following,
    required this.tickets,
  });

  /// Which of them the reader follows: `TicketView` is the same for every reader.
  final List<String> following;
  final List<TicketView> tickets;

  factory MyTicketsView.fromJson(Map<String, dynamic> json) => MyTicketsView(
    following: (json['following'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    tickets: (json['tickets'] as List<dynamic>).map((x0) => TicketView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'following': following,
    'tickets': tickets.map((x0) => x0.toJson()).toList(),
  };
}

final class NotificationView {
  const NotificationView({
    this.conversationId,
    required this.createdAtMs,
    required this.id,
    this.mergedCount,
    this.messageId,
    this.needsYou,
    required this.read,
    this.reason,
    required this.severity,
    required this.subject,
    required this.targetType,
    this.taskId,
    this.ticketId,
    this.ticketNumber,
    required this.title,
  });

  /// The channel or discussion the row is about (feature `personal_inbox`).
  final String? conversationId;
  final int createdAtMs;
  final String id;
  /// How many later events this one row stands for (feature `personal_inbox`).
  /// Left out when none.
  final int? mergedCount;
  /// The message the row is about (feature `personal_inbox`).
  final String? messageId;
  /// The row waits for the reader, worked out when it is read so that it follows
  /// a handover (feature `personal_inbox`). Left out when false.
  final bool? needsYou;
  final bool read;
  /// Why the reader gets the row (feature `personal_inbox`): `responsible`,
  /// `mention`, `plan_review`, `review`, `sent_back`, `failed` or `login`. Text,
  /// not an enum: a client shows an unknown reason as a plain row, so a new one
  /// never breaks `notification.list` on an older client.
  final String? reason;
  /// action | warning | info
  final String severity;
  final String subject;
  final String targetType;
  final String? taskId;
  /// The ticket the row is about (feature `personal_inbox`). Left out otherwise and by older servers.
  final String? ticketId;
  /// The number of `ticket_id` (`#12`).
  final int? ticketNumber;
  final String title;

  factory NotificationView.fromJson(Map<String, dynamic> json) => NotificationView(
    conversationId: (json['conversation_id'] == null ? null : (json['conversation_id'] as String)),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    id: (json['id'] as String),
    mergedCount: (json['merged_count'] == null ? null : (json['merged_count'] as num).toInt()),
    messageId: (json['message_id'] == null ? null : (json['message_id'] as String)),
    needsYou: (json['needs_you'] == null ? null : (json['needs_you'] as bool)),
    read: (json['read'] as bool),
    reason: (json['reason'] == null ? null : (json['reason'] as String)),
    severity: (json['severity'] as String),
    subject: (json['subject'] as String),
    targetType: (json['target_type'] as String),
    taskId: (json['task_id'] == null ? null : (json['task_id'] as String)),
    ticketId: (json['ticket_id'] == null ? null : (json['ticket_id'] as String)),
    ticketNumber: (json['ticket_number'] == null ? null : (json['ticket_number'] as num).toInt()),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (conversationId != null) 'conversation_id': conversationId,
    'created_at_ms': createdAtMs,
    'id': id,
    if (mergedCount != null) 'merged_count': mergedCount,
    if (messageId != null) 'message_id': messageId,
    if (needsYou != null) 'needs_you': needsYou,
    'read': read,
    if (reason != null) 'reason': reason,
    'severity': severity,
    'subject': subject,
    'target_type': targetType,
    if (taskId != null) 'task_id': taskId,
    if (ticketId != null) 'ticket_id': ticketId,
    if (ticketNumber != null) 'ticket_number': ticketNumber,
    'title': title,
  };
}

final class NotificationsSnapshot {
  const NotificationsSnapshot({
    required this.notifications,
    required this.unread,
  });

  final List<NotificationView> notifications;
  final int unread;

  factory NotificationsSnapshot.fromJson(Map<String, dynamic> json) => NotificationsSnapshot(
    notifications: (json['notifications'] as List<dynamic>).map((x0) => NotificationView.fromJson(x0 as Map<String, dynamic>)).toList(),
    unread: (json['unread'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'notifications': notifications.map((x0) => x0.toJson()).toList(),
    'unread': unread,
  };
}

/// `employee.open_chat`: the acting board member's lasting chat with this employee,
/// made the first time (feature `employee_chats`).
final class OpenEmployeeChat {
  const OpenEmployeeChat({
    required this.employeeId,
  });

  final String employeeId;

  factory OpenEmployeeChat.fromJson(Map<String, dynamic> json) => OpenEmployeeChat(
    employeeId: (json['employee_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'employee_id': employeeId,
  };
}

sealed class Outcome {
  const Outcome();

  factory Outcome.fromJson(Object? json) {
    if (json is Map<String, dynamic> && json.containsKey('error')) {
      return OutcomeError(value: AppError.fromJson(json['error'] as Map<String, dynamic>));
    }
    if (json is Map<String, dynamic> && json.containsKey('ok')) {
      return OutcomeOk(value: ResponseBody.fromJson(json['ok'] as Map<String, dynamic>));
    }
    throw FormatException('Unknown Outcome: $json');
  }

  Object toJson();
}

final class OutcomeError extends Outcome {
  const OutcomeError({
    required this.value,
  });

  final AppError value;

  @override
  Object toJson() => <String, dynamic>{
      'error': value.toJson(),
    };
}

final class OutcomeOk extends Outcome {
  const OutcomeOk({
    required this.value,
  });

  final ResponseBody value;

  @override
  Object toJson() => <String, dynamic>{
      'ok': value.toJson(),
    };
}

/// A fresh one-time pairing code; `qr` is the JSON the desktop renders as a QR code.
final class PairingCode {
  const PairingCode({
    required this.code,
    required this.expiresAtMs,
    required this.pairingText,
    required this.qr,
  });

  final String code;
  final int expiresAtMs;
  /// The same pairing as text to paste into Aurelius on another computer
  /// (`aurelius-pair:` + base64url of the payload JSON).
  final String pairingText;
  final String qr;

  factory PairingCode.fromJson(Map<String, dynamic> json) => PairingCode(
    code: (json['code'] as String),
    expiresAtMs: (json['expires_at_ms'] as num).toInt(),
    pairingText: (json['pairing_text'] as String),
    qr: (json['qr'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'code': code,
    'expires_at_ms': expiresAtMs,
    'pairing_text': pairingText,
    'qr': qr,
  };
}

final class PairingRequestView {
  const PairingRequestView({
    required this.deviceName,
    required this.owner,
    required this.pairingId,
    required this.platform,
    required this.requestedAtMs,
  });

  final String deviceName;
  /// A computer asking for full access (owner).
  final bool owner;
  final String pairingId;
  final String platform;
  final int requestedAtMs;

  factory PairingRequestView.fromJson(Map<String, dynamic> json) => PairingRequestView(
    deviceName: (json['device_name'] as String),
    owner: (json['owner'] as bool),
    pairingId: (json['pairing_id'] as String),
    platform: (json['platform'] as String),
    requestedAtMs: (json['requested_at_ms'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'device_name': deviceName,
    'owner': owner,
    'pairing_id': pairingId,
    'platform': platform,
    'requested_at_ms': requestedAtMs,
  };
}

/// A requested shutdown of the server's PC.
final class PendingShutdown {
  const PendingShutdown({
    this.deadlineMs,
    required this.requestedAtMs,
    required this.when,
  });

  /// When the PC powers off; set while the countdown runs, `None` while
  /// `when_idle` still waits for work to finish.
  final int? deadlineMs;
  final int requestedAtMs;
  final ShutdownWhen when;

  factory PendingShutdown.fromJson(Map<String, dynamic> json) => PendingShutdown(
    deadlineMs: (json['deadline_ms'] == null ? null : (json['deadline_ms'] as num).toInt()),
    requestedAtMs: (json['requested_at_ms'] as num).toInt(),
    when: ShutdownWhen.fromJson(json['when'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (deadlineMs != null) 'deadline_ms': deadlineMs,
    'requested_at_ms': requestedAtMs,
    'when': when.toJson(),
  };
}

final class PinMemory {
  const PinMemory({
    required this.memoryId,
    required this.pinned,
  });

  final String memoryId;
  final bool pinned;

  factory PinMemory.fromJson(Map<String, dynamic> json) => PinMemory(
    memoryId: (json['memory_id'] as String),
    pinned: (json['pinned'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'memory_id': memoryId,
    'pinned': pinned,
  };
}

/// How a plan is approved: carried out in Auto, or with edits accepted one by one.
enum PlanApproval {
  approve('approve'),
  approveEdits('approve_edits');

  const PlanApproval(this.wire);

  final String wire;

  static PlanApproval fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown PlanApproval value: $value'),
      );

  String toJson() => wire;
}

/// What the user decided about a plan Claude Code proposed (ExitPlanMode).
enum PlanDecision {
  /// Go ahead; the work continues in auto mode.
  approve('approve'),
  /// Go ahead; edits go through, everything else is asked (accept-edits mode).
  approveEdits('approve_edits'),
  /// No: the agent stops (feedback optional).
  reject('reject'),
  /// Not yet: the agent revises the plan with the user's feedback (required).
  revise('revise');

  const PlanDecision(this.wire);

  final String wire;

  static PlanDecision fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown PlanDecision value: $value'),
      );

  String toJson() => wire;
}

final class PlanLimitView {
  const PlanLimitView({
    required this.label,
    required this.percent,
    this.resets,
  });

  final String label;
  final int percent;
  final String? resets;

  factory PlanLimitView.fromJson(Map<String, dynamic> json) => PlanLimitView(
    label: (json['label'] as String),
    percent: (json['percent'] as num).toInt(),
    resets: (json['resets'] == null ? null : (json['resets'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'label': label,
    'percent': percent,
    if (resets != null) 'resets': resets,
  };
}

/// `power.get`, `power.shutdown`, `power.cancel` and `power_changed`.
final class PowerState {
  const PowerState({
    this.pending,
  });

  final PendingShutdown? pending;

  factory PowerState.fromJson(Map<String, dynamic> json) => PowerState(
    pending: (json['pending'] == null ? null : PendingShutdown.fromJson(json['pending'] as Map<String, dynamic>)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (pending != null) 'pending': pending?.toJson(),
  };
}

/// A conversation in a project's Conversations tab.
final class ProjectConversationSummary {
  const ProjectConversationSummary({
    required this.createdAtMs,
    required this.id,
    required this.messageCount,
    required this.title,
    required this.updatedAtMs,
    required this.working,
  });

  final int createdAtMs;
  final String id;
  final int messageCount;
  final String title;
  final int updatedAtMs;
  /// An answer is queued or running in it.
  final bool working;

  factory ProjectConversationSummary.fromJson(Map<String, dynamic> json) => ProjectConversationSummary(
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    id: (json['id'] as String),
    messageCount: (json['message_count'] as num).toInt(),
    title: (json['title'] as String),
    updatedAtMs: (json['updated_at_ms'] as num).toInt(),
    working: (json['working'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'created_at_ms': createdAtMs,
    'id': id,
    'message_count': messageCount,
    'title': title,
    'updated_at_ms': updatedAtMs,
    'working': working,
  };
}

final class ProjectConversationsSnapshot {
  const ProjectConversationsSnapshot({
    required this.asOfSeq,
    required this.conversations,
    required this.projectId,
  });

  final int asOfSeq;
  /// Most recently active first.
  final List<ProjectConversationSummary> conversations;
  final String projectId;

  factory ProjectConversationsSnapshot.fromJson(Map<String, dynamic> json) => ProjectConversationsSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    conversations: (json['conversations'] as List<dynamic>).map((x0) => ProjectConversationSummary.fromJson(x0 as Map<String, dynamic>)).toList(),
    projectId: (json['project_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'conversations': conversations.map((x0) => x0.toJson()).toList(),
    'project_id': projectId,
  };
}

enum ProjectStatus {
  active('active'),
  archived('archived'),
  paused('paused');

  const ProjectStatus(this.wire);

  final String wire;

  static ProjectStatus fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown ProjectStatus value: $value'),
      );

  String toJson() => wire;
}

final class ProjectSummary {
  const ProjectSummary({
    required this.createdAtMs,
    this.description,
    this.dispatcherEmployeeId,
    required this.employeesActive,
    this.folder,
    required this.id,
    required this.name,
    required this.status,
    this.styleguideUrl,
    required this.version,
  });

  final int createdAtMs;
  final String? description;
  /// The project's own dispatcher for tickets; none: the company's (B5).
  final String? dispatcherEmployeeId;
  /// Employees with an open task in this project (derived; employees are company-wide).
  final int employeesActive;
  /// The project's own folder on disk.
  final String? folder;
  final String id;
  final String name;
  final ProjectStatus status;
  /// The workspace's styleguide, a Claude Design artifact (feature `design_links`). A
  /// link only: runs read the artifact themselves. Left out when none and by older servers.
  final String? styleguideUrl;
  final int version;

  factory ProjectSummary.fromJson(Map<String, dynamic> json) => ProjectSummary(
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    description: (json['description'] == null ? null : (json['description'] as String)),
    dispatcherEmployeeId: (json['dispatcher_employee_id'] == null ? null : (json['dispatcher_employee_id'] as String)),
    employeesActive: (json['employees_active'] as num).toInt(),
    folder: (json['folder'] == null ? null : (json['folder'] as String)),
    id: (json['id'] as String),
    name: (json['name'] as String),
    status: ProjectStatus.fromJson(json['status'] as String),
    styleguideUrl: (json['styleguide_url'] == null ? null : (json['styleguide_url'] as String)),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'created_at_ms': createdAtMs,
    if (description != null) 'description': description,
    if (dispatcherEmployeeId != null) 'dispatcher_employee_id': dispatcherEmployeeId,
    'employees_active': employeesActive,
    if (folder != null) 'folder': folder,
    'id': id,
    'name': name,
    'status': status.toJson(),
    if (styleguideUrl != null) 'styleguide_url': styleguideUrl,
    'version': version,
  };
}

final class ProjectsSnapshot {
  const ProjectsSnapshot({
    required this.asOfSeq,
    required this.projects,
  });

  /// Subscribe after this cursor to stay consistent.
  final int asOfSeq;
  final List<ProjectSummary> projects;

  factory ProjectsSnapshot.fromJson(Map<String, dynamic> json) => ProjectsSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    projects: (json['projects'] as List<dynamic>).map((x0) => ProjectSummary.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'projects': projects.map((x0) => x0.toJson()).toList(),
  };
}

/// One reference an instruction proposal rests on: a task or a memory (a lesson).
final class ProposalEvidence {
  const ProposalEvidence({
    required this.id,
    required this.kind,
    required this.label,
  });

  /// The task's or the memory's id.
  final String id;
  /// `task` | `memory`
  final String kind;
  /// The task's title or the memory's text (shortened), as the user sees it.
  final String label;

  factory ProposalEvidence.fromJson(Map<String, dynamic> json) => ProposalEvidence(
    id: (json['id'] as String),
    kind: (json['kind'] as String),
    label: (json['label'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'id': id,
    'kind': kind,
    'label': label,
  };
}

/// The employee that would be created.
final class ProposalFields {
  const ProposalFields({
    required this.instructions,
    required this.name,
    this.reportsTo,
    required this.role,
    this.title,
  });

  final String instructions;
  final String name;
  /// Who the new employee reports to; the proposer unless said otherwise.
  final String? reportsTo;
  final String role;
  final String? title;

  factory ProposalFields.fromJson(Map<String, dynamic> json) => ProposalFields(
    instructions: (json['instructions'] as String),
    name: (json['name'] as String),
    reportsTo: (json['reports_to'] == null ? null : (json['reports_to'] as String)),
    role: (json['role'] as String),
    title: (json['title'] == null ? null : (json['title'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'instructions': instructions,
    'name': name,
    if (reportsTo != null) 'reports_to': reportsTo,
    'role': role,
    if (title != null) 'title': title,
  };
}

/// What a proposal is about.
enum ProposalKind {
  /// A new colleague (`propose_colleague`).
  colleague('colleague'),
  /// New instructions for an existing employee (`propose_instructions`).
  instructions('instructions');

  const ProposalKind(this.wire);

  final String wire;

  static ProposalKind fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown ProposalKind value: $value'),
      );

  String toJson() => wire;
}

enum ProposalStatus {
  approved('approved'),
  pending('pending'),
  rejected('rejected'),
  /// The proposer was archived before the user decided.
  withdrawn('withdrawn');

  const ProposalStatus(this.wire);

  final String wire;

  static ProposalStatus fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown ProposalStatus value: $value'),
      );

  String toJson() => wire;
}

/// A colleague an employee proposed; the user decides.
final class ProposalView {
  const ProposalView({
    this.baseVersion,
    required this.createdAtMs,
    this.createdEmployeeId,
    this.currentInstructions,
    this.currentVersion,
    this.decidedAtMs,
    this.decisionNote,
    this.evidence = const [],
    required this.id,
    required this.instructions,
    this.kind,
    required this.name,
    required this.proposedBy,
    required this.proposedByName,
    required this.reason,
    this.reportsTo,
    this.reportsToName,
    required this.role,
    required this.status,
    this.targetEmployeeId,
    this.targetEmployeeName,
    this.title,
  });

  /// The version of the target's instructions the proposal was made on.
  final int? baseVersion;
  final int createdAtMs;
  /// The employee created when the proposal was approved.
  final String? createdEmployeeId;
  /// The target's instructions now (show the diff against `instructions`).
  final String? currentInstructions;
  /// The target's current version; when it differs from `base_version`, the
  /// instructions changed since and approving is refused (`version_conflict`).
  final int? currentVersion;
  final int? decidedAtMs;
  /// The user's note on the decision (it goes back to the proposer).
  final String? decisionNote;
  /// The tasks and memories (lessons) the proposal rests on.
  final List<ProposalEvidence> evidence;
  final String id;
  final String instructions;
  /// What it proposes (feature `instruction_proposals`; older servers: a colleague).
  final ProposalKind? kind;
  final String name;
  final String proposedBy;
  final String proposedByName;
  /// Why the proposer wants this colleague.
  final String reason;
  final String? reportsTo;
  final String? reportsToName;
  final String role;
  final ProposalStatus status;
  /// Instruction proposals: the employee whose instructions would change (the
  /// proposer itself or a direct report). `name`/`role` are that employee's,
  /// `instructions` the proposed text.
  final String? targetEmployeeId;
  final String? targetEmployeeName;
  final String? title;

  factory ProposalView.fromJson(Map<String, dynamic> json) => ProposalView(
    baseVersion: (json['base_version'] == null ? null : (json['base_version'] as num).toInt()),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    createdEmployeeId: (json['created_employee_id'] == null ? null : (json['created_employee_id'] as String)),
    currentInstructions: (json['current_instructions'] == null ? null : (json['current_instructions'] as String)),
    currentVersion: (json['current_version'] == null ? null : (json['current_version'] as num).toInt()),
    decidedAtMs: (json['decided_at_ms'] == null ? null : (json['decided_at_ms'] as num).toInt()),
    decisionNote: (json['decision_note'] == null ? null : (json['decision_note'] as String)),
    evidence: (json['evidence'] == null ? const [] : (json['evidence'] as List<dynamic>).map((x0) => ProposalEvidence.fromJson(x0 as Map<String, dynamic>)).toList()),
    id: (json['id'] as String),
    instructions: (json['instructions'] as String),
    kind: (json['kind'] == null ? null : ProposalKind.fromJson(json['kind'] as String)),
    name: (json['name'] as String),
    proposedBy: (json['proposed_by'] as String),
    proposedByName: (json['proposed_by_name'] as String),
    reason: (json['reason'] as String),
    reportsTo: (json['reports_to'] == null ? null : (json['reports_to'] as String)),
    reportsToName: (json['reports_to_name'] == null ? null : (json['reports_to_name'] as String)),
    role: (json['role'] as String),
    status: ProposalStatus.fromJson(json['status'] as String),
    targetEmployeeId: (json['target_employee_id'] == null ? null : (json['target_employee_id'] as String)),
    targetEmployeeName: (json['target_employee_name'] == null ? null : (json['target_employee_name'] as String)),
    title: (json['title'] == null ? null : (json['title'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (baseVersion != null) 'base_version': baseVersion,
    'created_at_ms': createdAtMs,
    if (createdEmployeeId != null) 'created_employee_id': createdEmployeeId,
    if (currentInstructions != null) 'current_instructions': currentInstructions,
    if (currentVersion != null) 'current_version': currentVersion,
    if (decidedAtMs != null) 'decided_at_ms': decidedAtMs,
    if (decisionNote != null) 'decision_note': decisionNote,
    'evidence': evidence.map((x0) => x0.toJson()).toList(),
    'id': id,
    'instructions': instructions,
    if (kind != null) 'kind': kind?.toJson(),
    'name': name,
    'proposed_by': proposedBy,
    'proposed_by_name': proposedByName,
    'reason': reason,
    if (reportsTo != null) 'reports_to': reportsTo,
    if (reportsToName != null) 'reports_to_name': reportsToName,
    'role': role,
    'status': status.toJson(),
    if (targetEmployeeId != null) 'target_employee_id': targetEmployeeId,
    if (targetEmployeeName != null) 'target_employee_name': targetEmployeeName,
    if (title != null) 'title': title,
  };
}

final class ProposalsSnapshot {
  const ProposalsSnapshot({
    required this.asOfSeq,
    required this.proposals,
  });

  final int asOfSeq;
  /// Pending first, then the latest decisions.
  final List<ProposalView> proposals;

  factory ProposalsSnapshot.fromJson(Map<String, dynamic> json) => ProposalsSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    proposals: (json['proposals'] as List<dynamic>).map((x0) => ProposalView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'proposals': proposals.map((x0) => x0.toJson()).toList(),
  };
}

enum ProviderKind {
  /// Anthropic Messages API with the user's own API key.
  anthropicApi('anthropic_api'),
  /// Unmodified Claude Code CLI; the user signs in with Anthropic's own flow.
  claudeCode('claude_code'),
  /// Built-in deterministic test runtime; no network, no cost.
  fake('fake'),
  /// OpenAI Responses API with the user's own API key.
  openaiApi('openai_api');

  const ProviderKind(this.wire);

  final String wire;

  static ProviderKind fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown ProviderKind value: $value'),
      );

  String toJson() => wire;
}

enum Purpose {
  employee('employee'),
  memory('memory'),
  /// The subagents a Claude Code run starts itself.
  subagent('subagent');

  const Purpose(this.wire);

  final String wire;

  static Purpose fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown Purpose value: $value'),
      );

  String toJson() => wire;
}

/// `push.configure`. `key_p8` is the APNs auth key file content; none keeps
/// the stored key. The key goes to the OS credential store, never to SQLite.
final class PushConfig {
  const PushConfig({
    required this.bundleId,
    required this.keyId,
    this.keyP8,
    required this.teamId,
  });

  final String bundleId;
  final String keyId;
  final String? keyP8;
  final String teamId;

  factory PushConfig.fromJson(Map<String, dynamic> json) => PushConfig(
    bundleId: (json['bundle_id'] as String),
    keyId: (json['key_id'] as String),
    keyP8: (json['key_p8'] == null ? null : (json['key_p8'] as String)),
    teamId: (json['team_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'bundle_id': bundleId,
    'key_id': keyId,
    if (keyP8 != null) 'key_p8': keyP8,
    'team_id': teamId,
  };
}

final class PushStatus {
  const PushStatus({
    required this.apns,
    this.bundleId,
    this.keyId,
    this.lastError,
    this.teamId,
  });

  /// An APNs key is stored and the ids are set.
  final bool apns;
  final String? bundleId;
  final String? keyId;
  final String? lastError;
  final String? teamId;

  factory PushStatus.fromJson(Map<String, dynamic> json) => PushStatus(
    apns: (json['apns'] as bool),
    bundleId: (json['bundle_id'] == null ? null : (json['bundle_id'] as String)),
    keyId: (json['key_id'] == null ? null : (json['key_id'] as String)),
    lastError: (json['last_error'] == null ? null : (json['last_error'] as String)),
    teamId: (json['team_id'] == null ? null : (json['team_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'apns': apns,
    if (bundleId != null) 'bundle_id': bundleId,
    if (keyId != null) 'key_id': keyId,
    if (lastError != null) 'last_error': lastError,
    if (teamId != null) 'team_id': teamId,
  };
}

/// One answered question of a run (`RunDecision::answers`).
final class QuestionAnswer {
  const QuestionAnswer({
    required this.answer,
    required this.question,
  });

  final String answer;
  final String question;

  factory QuestionAnswer.fromJson(Map<String, dynamic> json) => QuestionAnswer(
    answer: (json['answer'] as String),
    question: (json['question'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'answer': answer,
    'question': question,
  };
}

/// A board member adds (`on`) or takes back their reaction with `emoji` on a
/// message of a channel or a ticket's discussion that is not closed
/// (`message.react`, feature `chat_reactions`). Idempotent: on twice is on once.
final class ReactToMessage {
  const ReactToMessage({
    required this.emoji,
    required this.messageId,
    required this.on,
  });

  final String emoji;
  final String messageId;
  final bool on;

  factory ReactToMessage.fromJson(Map<String, dynamic> json) => ReactToMessage(
    emoji: (json['emoji'] as String),
    messageId: (json['message_id'] as String),
    on: (json['on'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'emoji': emoji,
    'message_id': messageId,
    'on': on,
  };
}

/// One emoji on a message (`MessageView::reactions`, feature `chat_reactions`):
/// how many reacted with it, whether the reader did, and who.
final class ReactionView {
  const ReactionView({
    this.by = const [],
    required this.count,
    required this.emoji,
    required this.mine,
  });

  /// The names of who reacted (board members' usernames, employees' names), oldest first.
  final List<String> by;
  final int count;
  final String emoji;
  /// The board member who reads the message reacted with it.
  final bool mine;

  factory ReactionView.fromJson(Map<String, dynamic> json) => ReactionView(
    by: (json['by'] == null ? const [] : (json['by'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    count: (json['count'] as num).toInt(),
    emoji: (json['emoji'] as String),
    mine: (json['mine'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'by': by,
    'count': count,
    'emoji': emoji,
    'mine': mine,
  };
}

/// The answer to every write.
final class Receipt {
  const Receipt({
    required this.committedSeq,
    required this.idempotencyKey,
    required this.replayed,
    required this.result,
  });

  final int committedSeq;
  final String idempotencyKey;
  /// True when this is the receipt of an earlier identical write.
  final bool replayed;
  final WriteResult result;

  factory Receipt.fromJson(Map<String, dynamic> json) => Receipt(
    committedSeq: (json['committed_seq'] as num).toInt(),
    idempotencyKey: (json['idempotency_key'] as String),
    replayed: (json['replayed'] as bool),
    result: WriteResult.fromJson(json['result'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'committed_seq': committedSeq,
    'idempotency_key': idempotencyKey,
    'replayed': replayed,
    'result': result.toJson(),
  };
}

/// When a schedule is due. Weekdays count from Monday = 0 to Sunday = 6;
/// `at_minute` is minutes since local midnight (09:30 = 570).
sealed class Recurrence {
  const Recurrence();

  factory Recurrence.fromJson(Map<String, dynamic> json) {
    final tag = json['kind'] as String;
    switch (tag) {
      case 'daily':
        return RecurrenceDaily(
          atMinute: (json['at_minute'] as num).toInt(),
        );
      case 'every_hours':
        return RecurrenceEveryHours(
          hours: (json['hours'] as num).toInt(),
        );
      case 'weekly':
        return RecurrenceWeekly(
          atMinute: (json['at_minute'] as num).toInt(),
          weekdays: (json['weekdays'] as List<dynamic>).map((x0) => (x0 as num).toInt()).toList(),
        );
      default:
        throw FormatException('Unknown Recurrence kind: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

final class RecurrenceDaily extends Recurrence {
  const RecurrenceDaily({
    required this.atMinute,
  });

  final int atMinute;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'daily',
      'at_minute': atMinute,
    };
}

final class RecurrenceEveryHours extends Recurrence {
  const RecurrenceEveryHours({
    required this.hours,
  });

  final int hours;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'every_hours',
      'hours': hours,
    };
}

final class RecurrenceWeekly extends Recurrence {
  const RecurrenceWeekly({
    required this.atMinute,
    required this.weekdays,
  });

  final int atMinute;
  final List<int> weekdays;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'weekly',
      'at_minute': atMinute,
      'weekdays': weekdays,
    };
}

/// The user writes a memory directly: active at once, the command is its source.
final class Remember {
  const Remember({
    required this.content,
    required this.kind,
    this.pinned = false,
    required this.scope,
  });

  final String content;
  final MemoryKind kind;
  final bool pinned;
  final MemoryScope scope;

  factory Remember.fromJson(Map<String, dynamic> json) => Remember(
    content: (json['content'] as String),
    kind: MemoryKind.fromJson(json['kind'] as String),
    pinned: (json['pinned'] == null ? false : (json['pinned'] as bool)),
    scope: MemoryScope.fromJson(json['scope'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'content': content,
    'kind': kind.toJson(),
    'pinned': pinned,
    'scope': scope.toJson(),
  };
}

final class RemoteClientView {
  const RemoteClientView({
    required this.capabilities,
    required this.connected,
    required this.createdAtMs,
    required this.id,
    this.lastSeenMs,
    required this.name,
    required this.platform,
    required this.push,
  });

  final List<Capability> capabilities;
  final bool connected;
  final int createdAtMs;
  final String id;
  final int? lastSeenMs;
  final String name;
  /// ios | android | windows | macos | linux
  final String platform;
  /// The device registered for push notifications.
  final bool push;

  factory RemoteClientView.fromJson(Map<String, dynamic> json) => RemoteClientView(
    capabilities: (json['capabilities'] as List<dynamic>).map((x0) => Capability.fromJson(x0 as String)).toList(),
    connected: (json['connected'] as bool),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    id: (json['id'] as String),
    lastSeenMs: (json['last_seen_ms'] == null ? null : (json['last_seen_ms'] as num).toInt()),
    name: (json['name'] as String),
    platform: (json['platform'] as String),
    push: (json['push'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'capabilities': capabilities.map((x0) => x0.toJson()).toList(),
    'connected': connected,
    'created_at_ms': createdAtMs,
    'id': id,
    if (lastSeenMs != null) 'last_seen_ms': lastSeenMs,
    'name': name,
    'platform': platform,
    'push': push,
  };
}

/// Settings → Remote access.
final class RemoteStatus {
  const RemoteStatus({
    required this.clients,
    required this.enabled,
    this.error,
    required this.pending,
    required this.port,
    required this.push,
    required this.tailscale,
    this.url,
  });

  final List<RemoteClientView> clients;
  final bool enabled;
  final String? error;
  /// Devices that entered a valid code and wait for "Allow" on the desktop.
  final List<PairingRequestView> pending;
  /// Loopback port of the gateway (Tailscale Serve forwards to it).
  final int port;
  final PushStatus push;
  final TailscaleStatus tailscale;
  /// What a phone connects to, e.g. `https://desktop.tail1234.ts.net:8443`.
  final String? url;

  factory RemoteStatus.fromJson(Map<String, dynamic> json) => RemoteStatus(
    clients: (json['clients'] as List<dynamic>).map((x0) => RemoteClientView.fromJson(x0 as Map<String, dynamic>)).toList(),
    enabled: (json['enabled'] as bool),
    error: (json['error'] == null ? null : (json['error'] as String)),
    pending: (json['pending'] as List<dynamic>).map((x0) => PairingRequestView.fromJson(x0 as Map<String, dynamic>)).toList(),
    port: (json['port'] as num).toInt(),
    push: PushStatus.fromJson(json['push'] as Map<String, dynamic>),
    tailscale: TailscaleStatus.fromJson(json['tailscale'] as Map<String, dynamic>),
    url: (json['url'] == null ? null : (json['url'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'clients': clients.map((x0) => x0.toJson()).toList(),
    'enabled': enabled,
    if (error != null) 'error': error,
    'pending': pending.map((x0) => x0.toJson()).toList(),
    'port': port,
    'push': push.toJson(),
    'tailscale': tailscale.toJson(),
    if (url != null) 'url': url,
  };
}

final class RemoveAttachment {
  const RemoveAttachment({
    required this.attachmentId,
  });

  final String attachmentId;

  factory RemoveAttachment.fromJson(Map<String, dynamic> json) => RemoveAttachment(
    attachmentId: (json['attachment_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'attachment_id': attachmentId,
  };
}

final class RemoveConnection {
  const RemoveConnection({
    required this.connectionId,
  });

  final String connectionId;

  factory RemoveConnection.fromJson(Map<String, dynamic> json) => RemoveConnection(
    connectionId: (json['connection_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'connection_id': connectionId,
  };
}

final class RenameProject {
  const RenameProject({
    required this.name,
    required this.projectId,
  });

  final String name;
  final String projectId;

  factory RenameProject.fromJson(Map<String, dynamic> json) => RenameProject(
    name: (json['name'] as String),
    projectId: (json['project_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'name': name,
    'project_id': projectId,
  };
}

sealed class ResponseBody {
  const ResponseBody();

  factory ResponseBody.fromJson(Map<String, dynamic> json) {
    final tag = json['type'] as String;
    switch (tag) {
      case 'activity':
        return ResponseBodyActivity(value: ActivityView.fromJson(json['value'] as Map<String, dynamic>));
      case 'activity_log':
        return ResponseBodyActivityLog(value: ActivityLogPage.fromJson(json['value'] as Map<String, dynamic>));
      case 'activity_overview':
        return ResponseBodyActivityOverview(value: ActivityOverview.fromJson(json['value'] as Map<String, dynamic>));
      case 'all_ideas':
        return ResponseBodyAllIdeas(value: AllIdeasSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'approvals':
        return ResponseBodyApprovals(value: ApprovalsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'auto_allow_setting':
        return ResponseBodyAutoAllowSetting(value: AutoAllowSetting.fromJson(json['value'] as Map<String, dynamic>));
      case 'backups':
        return ResponseBodyBackups(value: BackupsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'board':
        return ResponseBodyBoard(value: BoardView.fromJson(json['value'] as Map<String, dynamic>));
      case 'browser':
        return ResponseBodyBrowser(value: BrowserSetting.fromJson(json['value'] as Map<String, dynamic>));
      case 'channels':
        return ResponseBodyChannels(value: ChannelsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'claude_login':
        return ResponseBodyClaudeLogin(value: ClaudeLoginView.fromJson(json['value'] as Map<String, dynamic>));
      case 'code':
        return ResponseBodyCode(value: CodeView.fromJson(json['value'] as Map<String, dynamic>));
      case 'command_applied':
        return ResponseBodyCommandApplied(value: CommandApplied.fromJson(json['value'] as Map<String, dynamic>));
      case 'compaction_setting':
        return ResponseBodyCompactionSetting(value: CompactionSetting.fromJson(json['value'] as Map<String, dynamic>));
      case 'company':
        return ResponseBodyCompany(value: CompanySnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'connections':
        return ResponseBodyConnections(value: ConnectionsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'context':
        return ResponseBodyContext(value: ContextView.fromJson(json['value'] as Map<String, dynamic>));
      case 'dir_listing':
        return ResponseBodyDirListing(value: DirListing.fromJson(json['value'] as Map<String, dynamic>));
      case 'discussion_audience':
        return ResponseBodyDiscussionAudience(value: DiscussionAudience.fromJson(json['value'] as Map<String, dynamic>));
      case 'effective_guidelines':
        return ResponseBodyEffectiveGuidelines(value: EffectiveGuidelines.fromJson(json['value'] as Map<String, dynamic>));
      case 'employee_chats':
        return ResponseBodyEmployeeChats(value: EmployeeChatsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'employee_instruction_versions':
        return ResponseBodyEmployeeInstructionVersions(value: EmployeeInstructionVersionsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'employee_queue':
        return ResponseBodyEmployeeQueue(value: EmployeeQueue.fromJson(json['value'] as Map<String, dynamic>));
      case 'employee_runs':
        return ResponseBodyEmployeeRuns(value: EmployeeRunsView.fromJson(json['value'] as Map<String, dynamic>));
      case 'employee_scratchpad':
        return ResponseBodyEmployeeScratchpad(value: ScratchpadView.fromJson(json['value'] as Map<String, dynamic>));
      case 'employee_start_profiles':
        return ResponseBodyEmployeeStartProfiles(value: StartProfiles.fromJson(json['value'] as Map<String, dynamic>));
      case 'employee_tool_options':
        return ResponseBodyEmployeeToolOptions(value: ToolOptions.fromJson(json['value'] as Map<String, dynamic>));
      case 'employees':
        return ResponseBodyEmployees(value: EmployeesSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'file':
        {
          final src = (json['value'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return ResponseBodyFile(
          dataBase64: (src['data_base64'] as String),
          id: (src['id'] as String),
          mediaType: (src['media_type'] as String),
          name: (src['name'] as String),
        );
        }
      case 'guideline':
        return ResponseBodyGuideline(value: GuidelineDetail.fromJson(json['value'] as Map<String, dynamic>));
      case 'guideline_affected':
        return ResponseBodyGuidelineAffected(value: GuidelineAffected.fromJson(json['value'] as Map<String, dynamic>));
      case 'guideline_proposals':
        return ResponseBodyGuidelineProposals(value: GuidelineProposals.fromJson(json['value'] as Map<String, dynamic>));
      case 'guideline_versions':
        return ResponseBodyGuidelineVersions(value: GuidelineVersions.fromJson(json['value'] as Map<String, dynamic>));
      case 'guidelines':
        return ResponseBodyGuidelines(value: GuidelinesSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'health':
        return ResponseBodyHealth(value: HealthResult.fromJson(json['value'] as Map<String, dynamic>));
      case 'hello':
        return ResponseBodyHello(value: HelloResult.fromJson(json['value'] as Map<String, dynamic>));
      case 'history':
        return ResponseBodyHistory(value: HistoryPage.fromJson(json['value'] as Map<String, dynamic>));
      case 'ideas':
        return ResponseBodyIdeas(value: IdeasSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'jobs':
        return ResponseBodyJobs(value: JobsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'load':
        return ResponseBodyLoad(value: LoadSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'login':
        return ResponseBodyLogin(value: LoginResult.fromJson(json['value'] as Map<String, dynamic>));
      case 'management_mode_setting':
        return ResponseBodyManagementModeSetting(value: ManagementModeSetting.fromJson(json['value'] as Map<String, dynamic>));
      case 'markdown_file':
        return ResponseBodyMarkdownFile(value: MarkdownFileContent.fromJson(json['value'] as Map<String, dynamic>));
      case 'markdown_files':
        return ResponseBodyMarkdownFiles(value: MarkdownFileList.fromJson(json['value'] as Map<String, dynamic>));
      case 'mcp_always_allow':
        return ResponseBodyMcpAlwaysAllow(value: McpAlwaysAllow.fromJson(json['value'] as Map<String, dynamic>));
      case 'mcp_login':
        return ResponseBodyMcpLogin(value: McpLogin.fromJson(json['value'] as Map<String, dynamic>));
      case 'mcp_servers':
        return ResponseBodyMcpServers(value: McpServers.fromJson(json['value'] as Map<String, dynamic>));
      case 'memory':
        return ResponseBodyMemory(value: MemorySnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'memory_context':
        return ResponseBodyMemoryContext(value: MemoryContextView.fromJson(json['value'] as Map<String, dynamic>));
      case 'message_history':
        return ResponseBodyMessageHistory(value: MessageHistoryView.fromJson(json['value'] as Map<String, dynamic>));
      case 'message_location':
        return ResponseBodyMessageLocation(value: MessageLocation.fromJson(json['value'] as Map<String, dynamic>));
      case 'messages':
        return ResponseBodyMessages(value: MessagesSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'my_tickets':
        return ResponseBodyMyTickets(value: MyTicketsView.fromJson(json['value'] as Map<String, dynamic>));
      case 'notifications':
        return ResponseBodyNotifications(value: NotificationsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'power':
        return ResponseBodyPower(value: PowerState.fromJson(json['value'] as Map<String, dynamic>));
      case 'project_conversations':
        return ResponseBodyProjectConversations(value: ProjectConversationsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'projects':
        return ResponseBodyProjects(value: ProjectsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'proposals':
        return ResponseBodyProposals(value: ProposalsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'receipt':
        return ResponseBodyReceipt(value: Receipt.fromJson(json['value'] as Map<String, dynamic>));
      case 'remote':
        return ResponseBodyRemote(value: RemoteStatus.fromJson(json['value'] as Map<String, dynamic>));
      case 'run_activity':
        return ResponseBodyRunActivity(value: RunActivity.fromJson(json['value'] as Map<String, dynamic>));
      case 'run_guidelines':
        return ResponseBodyRunGuidelines(value: RunGuidelines.fromJson(json['value'] as Map<String, dynamic>));
      case 'run_trace':
        return ResponseBodyRunTrace(value: RunTrace.fromJson(json['value'] as Map<String, dynamic>));
      case 'runtime_detected':
        {
          final src = (json['value'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return ResponseBodyRuntimeDetected(
          claudeCode: (src['claude_code'] == null ? null : (src['claude_code'] as String)),
        );
        }
      case 'scheduled_messages':
        return ResponseBodyScheduledMessages(value: ScheduledMessagesSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'screenshot':
        {
          final src = (json['value'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return ResponseBodyScreenshot(
          id: (src['id'] as String),
          jpegBase64: (src['jpeg_base64'] as String),
        );
        }
      case 'server_name':
        return ResponseBodyServerName(value: ServerNameView.fromJson(json['value'] as Map<String, dynamic>));
      case 'session_history':
        return ResponseBodySessionHistory(value: SessionHistoryPage.fromJson(json['value'] as Map<String, dynamic>));
      case 'sessions':
        return ResponseBodySessions(value: SessionsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'starter_set':
        return ResponseBodyStarterSet(value: StarterSet.fromJson(json['value'] as Map<String, dynamic>));
      case 'subagents':
        return ResponseBodySubagents(value: (json['value'] as List<dynamic>).map((x0) => AgentView.fromJson(x0 as Map<String, dynamic>)).toList());
      case 'subscribed':
        {
          final src = (json['value'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
          return ResponseBodySubscribed(
          replayedThroughSeq: (src['replayed_through_seq'] as num).toInt(),
        );
        }
      case 'subscription':
        return ResponseBodySubscription(value: SubscriptionUsage.fromJson(json['value'] as Map<String, dynamic>));
      case 'task_schedules':
        return ResponseBodyTaskSchedules(value: TaskSchedulesSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'tasks':
        return ResponseBodyTasks(value: TasksSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'thread':
        return ResponseBodyThread(value: ThreadSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'thread_waiting':
        return ResponseBodyThreadWaiting(value: ThreadWaitingSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'ticket':
        return ResponseBodyTicket(value: TicketDetail.fromJson(json['value'] as Map<String, dynamic>));
      case 'ticket_followers':
        return ResponseBodyTicketFollowers(value: TicketFollowersView.fromJson(json['value'] as Map<String, dynamic>));
      case 'tickets':
        return ResponseBodyTickets(value: TicketsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'timeline':
        return ResponseBodyTimeline(value: TimelinePage.fromJson(json['value'] as Map<String, dynamic>));
      case 'trace_setting':
        return ResponseBodyTraceSetting(value: TraceSetting.fromJson(json['value'] as Map<String, dynamic>));
      case 'update':
        return ResponseBodyUpdate(value: UpdateStatus.fromJson(json['value'] as Map<String, dynamic>));
      case 'upload':
        return ResponseBodyUpload(value: UploadChunk.fromJson(json['value'] as Map<String, dynamic>));
      case 'usage':
        return ResponseBodyUsage(value: UsageSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'usage_hours':
        return ResponseBodyUsageHours(value: UsageHours.fromJson(json['value'] as Map<String, dynamic>));
      case 'usage_totals':
        return ResponseBodyUsageTotals(value: UsageTotals.fromJson(json['value'] as Map<String, dynamic>));
      case 'users':
        return ResponseBodyUsers(value: UsersSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      case 'vault':
        return ResponseBodyVault(value: VaultList.fromJson(json['value'] as Map<String, dynamic>));
      case 'vault_audit':
        return ResponseBodyVaultAudit(value: VaultAuditPage.fromJson(json['value'] as Map<String, dynamic>));
      case 'vault_key':
        return ResponseBodyVaultKey(value: VaultKeyExport.fromJson(json['value'] as Map<String, dynamic>));
      case 'vault_value':
        return ResponseBodyVaultValue(value: VaultValue.fromJson(json['value'] as Map<String, dynamic>));
      case 'who_am_i':
        return ResponseBodyWhoAmI(value: WhoAmI.fromJson(json['value'] as Map<String, dynamic>));
      case 'work_sessions':
        return ResponseBodyWorkSessions(value: WorkSessionsSnapshot.fromJson(json['value'] as Map<String, dynamic>));
      default:
        throw FormatException('Unknown ResponseBody type: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

/// `activity.get`: one tool call whole.
final class ResponseBodyActivity extends ResponseBody {
  const ResponseBodyActivity({
    required this.value,
  });

  final ActivityView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'activity',
      'value': value.toJson(),
    };
}

/// `activity_log.list`.
final class ResponseBodyActivityLog extends ResponseBody {
  const ResponseBodyActivityLog({
    required this.value,
  });

  final ActivityLogPage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'activity_log',
      'value': value.toJson(),
    };
}

/// `activity.overview`.
final class ResponseBodyActivityOverview extends ResponseBody {
  const ResponseBodyActivityOverview({
    required this.value,
  });

  final ActivityOverview value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'activity_overview',
      'value': value.toJson(),
    };
}

final class ResponseBodyAllIdeas extends ResponseBody {
  const ResponseBodyAllIdeas({
    required this.value,
  });

  final AllIdeasSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'all_ideas',
      'value': value.toJson(),
    };
}

final class ResponseBodyApprovals extends ResponseBody {
  const ResponseBodyApprovals({
    required this.value,
  });

  final ApprovalsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'approvals',
      'value': value.toJson(),
    };
}

final class ResponseBodyAutoAllowSetting extends ResponseBody {
  const ResponseBodyAutoAllowSetting({
    required this.value,
  });

  final AutoAllowSetting value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'auto_allow_setting',
      'value': value.toJson(),
    };
}

final class ResponseBodyBackups extends ResponseBody {
  const ResponseBodyBackups({
    required this.value,
  });

  final BackupsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'backups',
      'value': value.toJson(),
    };
}

final class ResponseBodyBoard extends ResponseBody {
  const ResponseBodyBoard({
    required this.value,
  });

  final BoardView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'board',
      'value': value.toJson(),
    };
}

final class ResponseBodyBrowser extends ResponseBody {
  const ResponseBodyBrowser({
    required this.value,
  });

  final BrowserSetting value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'browser',
      'value': value.toJson(),
    };
}

final class ResponseBodyChannels extends ResponseBody {
  const ResponseBodyChannels({
    required this.value,
  });

  final ChannelsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'channels',
      'value': value.toJson(),
    };
}

final class ResponseBodyClaudeLogin extends ResponseBody {
  const ResponseBodyClaudeLogin({
    required this.value,
  });

  final ClaudeLoginView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'claude_login',
      'value': value.toJson(),
    };
}

/// `code.get`.
final class ResponseBodyCode extends ResponseBody {
  const ResponseBodyCode({
    required this.value,
  });

  final CodeView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'code',
      'value': value.toJson(),
    };
}

/// `command.applied`.
final class ResponseBodyCommandApplied extends ResponseBody {
  const ResponseBodyCommandApplied({
    required this.value,
  });

  final CommandApplied value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'command_applied',
      'value': value.toJson(),
    };
}

final class ResponseBodyCompactionSetting extends ResponseBody {
  const ResponseBodyCompactionSetting({
    required this.value,
  });

  final CompactionSetting value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'compaction_setting',
      'value': value.toJson(),
    };
}

final class ResponseBodyCompany extends ResponseBody {
  const ResponseBodyCompany({
    required this.value,
  });

  final CompanySnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'company',
      'value': value.toJson(),
    };
}

final class ResponseBodyConnections extends ResponseBody {
  const ResponseBodyConnections({
    required this.value,
  });

  final ConnectionsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'connections',
      'value': value.toJson(),
    };
}

final class ResponseBodyContext extends ResponseBody {
  const ResponseBodyContext({
    required this.value,
  });

  final ContextView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'context',
      'value': value.toJson(),
    };
}

final class ResponseBodyDirListing extends ResponseBody {
  const ResponseBodyDirListing({
    required this.value,
  });

  final DirListing value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'dir_listing',
      'value': value.toJson(),
    };
}

/// `discussion.audience`.
final class ResponseBodyDiscussionAudience extends ResponseBody {
  const ResponseBodyDiscussionAudience({
    required this.value,
  });

  final DiscussionAudience value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'discussion_audience',
      'value': value.toJson(),
    };
}

/// `guideline.effective`.
final class ResponseBodyEffectiveGuidelines extends ResponseBody {
  const ResponseBodyEffectiveGuidelines({
    required this.value,
  });

  final EffectiveGuidelines value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'effective_guidelines',
      'value': value.toJson(),
    };
}

final class ResponseBodyEmployeeChats extends ResponseBody {
  const ResponseBodyEmployeeChats({
    required this.value,
  });

  final EmployeeChatsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_chats',
      'value': value.toJson(),
    };
}

final class ResponseBodyEmployeeInstructionVersions extends ResponseBody {
  const ResponseBodyEmployeeInstructionVersions({
    required this.value,
  });

  final EmployeeInstructionVersionsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_instruction_versions',
      'value': value.toJson(),
    };
}

/// `employee.queue`.
final class ResponseBodyEmployeeQueue extends ResponseBody {
  const ResponseBodyEmployeeQueue({
    required this.value,
  });

  final EmployeeQueue value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_queue',
      'value': value.toJson(),
    };
}

final class ResponseBodyEmployeeRuns extends ResponseBody {
  const ResponseBodyEmployeeRuns({
    required this.value,
  });

  final EmployeeRunsView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_runs',
      'value': value.toJson(),
    };
}

final class ResponseBodyEmployeeScratchpad extends ResponseBody {
  const ResponseBodyEmployeeScratchpad({
    required this.value,
  });

  final ScratchpadView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_scratchpad',
      'value': value.toJson(),
    };
}

/// `employee.start_profiles`.
final class ResponseBodyEmployeeStartProfiles extends ResponseBody {
  const ResponseBodyEmployeeStartProfiles({
    required this.value,
  });

  final StartProfiles value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_start_profiles',
      'value': value.toJson(),
    };
}

/// `employee.tool_options`.
final class ResponseBodyEmployeeToolOptions extends ResponseBody {
  const ResponseBodyEmployeeToolOptions({
    required this.value,
  });

  final ToolOptions value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee_tool_options',
      'value': value.toJson(),
    };
}

final class ResponseBodyEmployees extends ResponseBody {
  const ResponseBodyEmployees({
    required this.value,
  });

  final EmployeesSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employees',
      'value': value.toJson(),
    };
}

/// A file kept with a message.
final class ResponseBodyFile extends ResponseBody {
  const ResponseBodyFile({
    required this.dataBase64,
    required this.id,
    required this.mediaType,
    required this.name,
  });

  final String dataBase64;
  final String id;
  final String mediaType;
  final String name;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'file',
      'value': <String, dynamic>{
      'data_base64': dataBase64,
      'id': id,
      'media_type': mediaType,
      'name': name,
    },
    };
}

/// `guideline.get`.
final class ResponseBodyGuideline extends ResponseBody {
  const ResponseBodyGuideline({
    required this.value,
  });

  final GuidelineDetail value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'guideline',
      'value': value.toJson(),
    };
}

/// `guideline.affected`.
final class ResponseBodyGuidelineAffected extends ResponseBody {
  const ResponseBodyGuidelineAffected({
    required this.value,
  });

  final GuidelineAffected value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'guideline_affected',
      'value': value.toJson(),
    };
}

/// `guideline.proposals`.
final class ResponseBodyGuidelineProposals extends ResponseBody {
  const ResponseBodyGuidelineProposals({
    required this.value,
  });

  final GuidelineProposals value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'guideline_proposals',
      'value': value.toJson(),
    };
}

/// `guideline.versions`.
final class ResponseBodyGuidelineVersions extends ResponseBody {
  const ResponseBodyGuidelineVersions({
    required this.value,
  });

  final GuidelineVersions value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'guideline_versions',
      'value': value.toJson(),
    };
}

/// `guideline.list`.
final class ResponseBodyGuidelines extends ResponseBody {
  const ResponseBodyGuidelines({
    required this.value,
  });

  final GuidelinesSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'guidelines',
      'value': value.toJson(),
    };
}

final class ResponseBodyHealth extends ResponseBody {
  const ResponseBodyHealth({
    required this.value,
  });

  final HealthResult value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'health',
      'value': value.toJson(),
    };
}

final class ResponseBodyHello extends ResponseBody {
  const ResponseBodyHello({
    required this.value,
  });

  final HelloResult value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'hello',
      'value': value.toJson(),
    };
}

/// `ticket.history`.
final class ResponseBodyHistory extends ResponseBody {
  const ResponseBodyHistory({
    required this.value,
  });

  final HistoryPage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'history',
      'value': value.toJson(),
    };
}

final class ResponseBodyIdeas extends ResponseBody {
  const ResponseBodyIdeas({
    required this.value,
  });

  final IdeasSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'ideas',
      'value': value.toJson(),
    };
}

final class ResponseBodyJobs extends ResponseBody {
  const ResponseBodyJobs({
    required this.value,
  });

  final JobsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'jobs',
      'value': value.toJson(),
    };
}

final class ResponseBodyLoad extends ResponseBody {
  const ResponseBodyLoad({
    required this.value,
  });

  final LoadSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'load',
      'value': value.toJson(),
    };
}

/// `user.login`: the session token, once.
final class ResponseBodyLogin extends ResponseBody {
  const ResponseBodyLogin({
    required this.value,
  });

  final LoginResult value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'login',
      'value': value.toJson(),
    };
}

final class ResponseBodyManagementModeSetting extends ResponseBody {
  const ResponseBodyManagementModeSetting({
    required this.value,
  });

  final ManagementModeSetting value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'management_mode_setting',
      'value': value.toJson(),
    };
}

final class ResponseBodyMarkdownFile extends ResponseBody {
  const ResponseBodyMarkdownFile({
    required this.value,
  });

  final MarkdownFileContent value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'markdown_file',
      'value': value.toJson(),
    };
}

final class ResponseBodyMarkdownFiles extends ResponseBody {
  const ResponseBodyMarkdownFiles({
    required this.value,
  });

  final MarkdownFileList value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'markdown_files',
      'value': value.toJson(),
    };
}

final class ResponseBodyMcpAlwaysAllow extends ResponseBody {
  const ResponseBodyMcpAlwaysAllow({
    required this.value,
  });

  final McpAlwaysAllow value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'mcp_always_allow',
      'value': value.toJson(),
    };
}

final class ResponseBodyMcpLogin extends ResponseBody {
  const ResponseBodyMcpLogin({
    required this.value,
  });

  final McpLogin value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'mcp_login',
      'value': value.toJson(),
    };
}

final class ResponseBodyMcpServers extends ResponseBody {
  const ResponseBodyMcpServers({
    required this.value,
  });

  final McpServers value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'mcp_servers',
      'value': value.toJson(),
    };
}

final class ResponseBodyMemory extends ResponseBody {
  const ResponseBodyMemory({
    required this.value,
  });

  final MemorySnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'memory',
      'value': value.toJson(),
    };
}

final class ResponseBodyMemoryContext extends ResponseBody {
  const ResponseBodyMemoryContext({
    required this.value,
  });

  final MemoryContextView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'memory_context',
      'value': value.toJson(),
    };
}

/// `message.history`.
final class ResponseBodyMessageHistory extends ResponseBody {
  const ResponseBodyMessageHistory({
    required this.value,
  });

  final MessageHistoryView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'message_history',
      'value': value.toJson(),
    };
}

/// `message.locate`.
final class ResponseBodyMessageLocation extends ResponseBody {
  const ResponseBodyMessageLocation({
    required this.value,
  });

  final MessageLocation value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'message_location',
      'value': value.toJson(),
    };
}

final class ResponseBodyMessages extends ResponseBody {
  const ResponseBodyMessages({
    required this.value,
  });

  final MessagesSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'messages',
      'value': value.toJson(),
    };
}

/// `ticket.my_tickets`.
final class ResponseBodyMyTickets extends ResponseBody {
  const ResponseBodyMyTickets({
    required this.value,
  });

  final MyTicketsView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'my_tickets',
      'value': value.toJson(),
    };
}

final class ResponseBodyNotifications extends ResponseBody {
  const ResponseBodyNotifications({
    required this.value,
  });

  final NotificationsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'notifications',
      'value': value.toJson(),
    };
}

final class ResponseBodyPower extends ResponseBody {
  const ResponseBodyPower({
    required this.value,
  });

  final PowerState value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'power',
      'value': value.toJson(),
    };
}

final class ResponseBodyProjectConversations extends ResponseBody {
  const ResponseBodyProjectConversations({
    required this.value,
  });

  final ProjectConversationsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'project_conversations',
      'value': value.toJson(),
    };
}

final class ResponseBodyProjects extends ResponseBody {
  const ResponseBodyProjects({
    required this.value,
  });

  final ProjectsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'projects',
      'value': value.toJson(),
    };
}

final class ResponseBodyProposals extends ResponseBody {
  const ResponseBodyProposals({
    required this.value,
  });

  final ProposalsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'proposals',
      'value': value.toJson(),
    };
}

/// The answer to every write.
final class ResponseBodyReceipt extends ResponseBody {
  const ResponseBodyReceipt({
    required this.value,
  });

  final Receipt value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'receipt',
      'value': value.toJson(),
    };
}

final class ResponseBodyRemote extends ResponseBody {
  const ResponseBodyRemote({
    required this.value,
  });

  final RemoteStatus value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'remote',
      'value': value.toJson(),
    };
}

final class ResponseBodyRunActivity extends ResponseBody {
  const ResponseBodyRunActivity({
    required this.value,
  });

  final RunActivity value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'run_activity',
      'value': value.toJson(),
    };
}

/// `run.guidelines`.
final class ResponseBodyRunGuidelines extends ResponseBody {
  const ResponseBodyRunGuidelines({
    required this.value,
  });

  final RunGuidelines value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'run_guidelines',
      'value': value.toJson(),
    };
}

final class ResponseBodyRunTrace extends ResponseBody {
  const ResponseBodyRunTrace({
    required this.value,
  });

  final RunTrace value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'run_trace',
      'value': value.toJson(),
    };
}

/// `None`: Claude Code was not found on the server.
final class ResponseBodyRuntimeDetected extends ResponseBody {
  const ResponseBodyRuntimeDetected({
    this.claudeCode,
  });

  final String? claudeCode;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'runtime_detected',
      'value': <String, dynamic>{
      if (claudeCode != null) 'claude_code': claudeCode,
    },
    };
}

final class ResponseBodyScheduledMessages extends ResponseBody {
  const ResponseBodyScheduledMessages({
    required this.value,
  });

  final ScheduledMessagesSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'scheduled_messages',
      'value': value.toJson(),
    };
}

/// A screenshot the agent took, as JPEG.
final class ResponseBodyScreenshot extends ResponseBody {
  const ResponseBodyScreenshot({
    required this.id,
    required this.jpegBase64,
  });

  final String id;
  final String jpegBase64;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'screenshot',
      'value': <String, dynamic>{
      'id': id,
      'jpeg_base64': jpegBase64,
    },
    };
}

/// `server.get_name`.
final class ResponseBodyServerName extends ResponseBody {
  const ResponseBodyServerName({
    required this.value,
  });

  final ServerNameView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'server_name',
      'value': value.toJson(),
    };
}

/// `work_session.history`.
final class ResponseBodySessionHistory extends ResponseBody {
  const ResponseBodySessionHistory({
    required this.value,
  });

  final SessionHistoryPage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'session_history',
      'value': value.toJson(),
    };
}

final class ResponseBodySessions extends ResponseBody {
  const ResponseBodySessions({
    required this.value,
  });

  final SessionsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'sessions',
      'value': value.toJson(),
    };
}

/// `guideline.starter_set`.
final class ResponseBodyStarterSet extends ResponseBody {
  const ResponseBodyStarterSet({
    required this.value,
  });

  final StarterSet value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'starter_set',
      'value': value.toJson(),
    };
}

final class ResponseBodySubagents extends ResponseBody {
  const ResponseBodySubagents({
    required this.value,
  });

  final List<AgentView> value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'subagents',
      'value': value.map((x0) => x0.toJson()).toList(),
    };
}

final class ResponseBodySubscribed extends ResponseBody {
  const ResponseBodySubscribed({
    required this.replayedThroughSeq,
  });

  final int replayedThroughSeq;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'subscribed',
      'value': <String, dynamic>{
      'replayed_through_seq': replayedThroughSeq,
    },
    };
}

final class ResponseBodySubscription extends ResponseBody {
  const ResponseBodySubscription({
    required this.value,
  });

  final SubscriptionUsage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'subscription',
      'value': value.toJson(),
    };
}

final class ResponseBodyTaskSchedules extends ResponseBody {
  const ResponseBodyTaskSchedules({
    required this.value,
  });

  final TaskSchedulesSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'task_schedules',
      'value': value.toJson(),
    };
}

final class ResponseBodyTasks extends ResponseBody {
  const ResponseBodyTasks({
    required this.value,
  });

  final TasksSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'tasks',
      'value': value.toJson(),
    };
}

/// `thread.read`.
final class ResponseBodyThread extends ResponseBody {
  const ResponseBodyThread({
    required this.value,
  });

  final ThreadSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'thread',
      'value': value.toJson(),
    };
}

final class ResponseBodyThreadWaiting extends ResponseBody {
  const ResponseBodyThreadWaiting({
    required this.value,
  });

  final ThreadWaitingSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'thread_waiting',
      'value': value.toJson(),
    };
}

final class ResponseBodyTicket extends ResponseBody {
  const ResponseBodyTicket({
    required this.value,
  });

  final TicketDetail value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'ticket',
      'value': value.toJson(),
    };
}

/// `ticket.followers`.
final class ResponseBodyTicketFollowers extends ResponseBody {
  const ResponseBodyTicketFollowers({
    required this.value,
  });

  final TicketFollowersView value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'ticket_followers',
      'value': value.toJson(),
    };
}

final class ResponseBodyTickets extends ResponseBody {
  const ResponseBodyTickets({
    required this.value,
  });

  final TicketsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'tickets',
      'value': value.toJson(),
    };
}

final class ResponseBodyTimeline extends ResponseBody {
  const ResponseBodyTimeline({
    required this.value,
  });

  final TimelinePage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'timeline',
      'value': value.toJson(),
    };
}

final class ResponseBodyTraceSetting extends ResponseBody {
  const ResponseBodyTraceSetting({
    required this.value,
  });

  final TraceSetting value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'trace_setting',
      'value': value.toJson(),
    };
}

final class ResponseBodyUpdate extends ResponseBody {
  const ResponseBodyUpdate({
    required this.value,
  });

  final UpdateStatus value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'update',
      'value': value.toJson(),
    };
}

/// A file the user uploaded (a part of it, or its thumbnail).
final class ResponseBodyUpload extends ResponseBody {
  const ResponseBodyUpload({
    required this.value,
  });

  final UploadChunk value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'upload',
      'value': value.toJson(),
    };
}

final class ResponseBodyUsage extends ResponseBody {
  const ResponseBodyUsage({
    required this.value,
  });

  final UsageSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'usage',
      'value': value.toJson(),
    };
}

final class ResponseBodyUsageHours extends ResponseBody {
  const ResponseBodyUsageHours({
    required this.value,
  });

  final UsageHours value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'usage_hours',
      'value': value.toJson(),
    };
}

final class ResponseBodyUsageTotals extends ResponseBody {
  const ResponseBodyUsageTotals({
    required this.value,
  });

  final UsageTotals value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'usage_totals',
      'value': value.toJson(),
    };
}

final class ResponseBodyUsers extends ResponseBody {
  const ResponseBodyUsers({
    required this.value,
  });

  final UsersSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'users',
      'value': value.toJson(),
    };
}

/// `vault.list`.
final class ResponseBodyVault extends ResponseBody {
  const ResponseBodyVault({
    required this.value,
  });

  final VaultList value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'vault',
      'value': value.toJson(),
    };
}

/// `vault.audit`.
final class ResponseBodyVaultAudit extends ResponseBody {
  const ResponseBodyVaultAudit({
    required this.value,
  });

  final VaultAuditPage value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'vault_audit',
      'value': value.toJson(),
    };
}

/// `vault.key_export`: never printed.
final class ResponseBodyVaultKey extends ResponseBody {
  const ResponseBodyVaultKey({
    required this.value,
  });

  final VaultKeyExport value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'vault_key',
      'value': value.toJson(),
    };
}

/// `vault.reveal`: never printed.
final class ResponseBodyVaultValue extends ResponseBody {
  const ResponseBodyVaultValue({
    required this.value,
  });

  final VaultValue value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'vault_value',
      'value': value.toJson(),
    };
}

final class ResponseBodyWhoAmI extends ResponseBody {
  const ResponseBodyWhoAmI({
    required this.value,
  });

  final WhoAmI value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'who_am_i',
      'value': value.toJson(),
    };
}

/// `work_session.list`.
final class ResponseBodyWorkSessions extends ResponseBody {
  const ResponseBodyWorkSessions({
    required this.value,
  });

  final WorkSessionsSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'work_sessions',
      'value': value.toJson(),
    };
}

/// Put an earlier version of an employee's instructions back. It becomes a new
/// version (history is never rewritten), and the employee's conversations start
/// a new session with it.
final class RestoreEmployeeInstructions {
  const RestoreEmployeeInstructions({
    required this.employeeId,
    required this.version,
  });

  final String employeeId;
  /// The version to put back (`EmployeeInstructionVersionView::version`).
  final int version;

  factory RestoreEmployeeInstructions.fromJson(Map<String, dynamic> json) => RestoreEmployeeInstructions(
    employeeId: (json['employee_id'] as String),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'employee_id': employeeId,
    'version': version,
  };
}

/// Ask for a new answer version; the previous answer is kept.
final class RetryReply {
  const RetryReply({
    required this.conversationId,
    required this.userMessageId,
  });

  final String conversationId;
  final String userMessageId;

  factory RetryReply.fromJson(Map<String, dynamic> json) => RetryReply(
    conversationId: (json['conversation_id'] as String),
    userMessageId: (json['user_message_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversation_id': conversationId,
    'user_message_id': userMessageId,
  };
}

/// A board member asks for a session's summary in the History now (feature
/// `session_history`): after it failed, for a session without one, or to bring
/// it up to date. Any board member may, as the History shows every session.
final class RetrySessionSummary {
  const RetrySessionSummary({
    required this.conversationId,
  });

  final String conversationId;

  factory RetrySessionSummary.fromJson(Map<String, dynamic> json) => RetrySessionSummary(
    conversationId: (json['conversation_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversation_id': conversationId,
  };
}

/// Why `vault.reveal` is asked: written to the audit log as `reveal` or `copy`.
enum RevealPurpose {
  copy('copy'),
  show('show');

  const RevealPurpose(this.wire);

  final String wire;

  static RevealPurpose fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown RevealPurpose value: $value'),
      );

  String toJson() => wire;
}

/// Replace the content with a new version; the old one is superseded, not erased.
final class ReviseMemory {
  const ReviseMemory({
    required this.content,
    required this.memoryId,
  });

  final String content;
  final String memoryId;

  factory ReviseMemory.fromJson(Map<String, dynamic> json) => ReviseMemory(
    content: (json['content'] as String),
    memoryId: (json['memory_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'content': content,
    'memory_id': memoryId,
  };
}

/// `run.get_activity`: every tool call of a run, for an opened run card (feature `run_cards`).
final class RunActivity {
  const RunActivity({
    required this.calls,
    this.messageId,
    required this.runId,
  });

  final List<ActivityView> calls;
  /// The answer the calls are kept with (`activity.get` reads one whole); none without calls.
  final String? messageId;
  final String runId;

  factory RunActivity.fromJson(Map<String, dynamic> json) => RunActivity(
    calls: (json['calls'] as List<dynamic>).map((x0) => ActivityView.fromJson(x0 as Map<String, dynamic>)).toList(),
    messageId: (json['message_id'] == null ? null : (json['message_id'] as String)),
    runId: (json['run_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'calls': calls.map((x0) => x0.toJson()).toList(),
    if (messageId != null) 'message_id': messageId,
    'run_id': runId,
  };
}

/// What the run card of a post shows while folded (`MessageView::run`).
final class RunBrief {
  const RunBrief({
    this.employeeId,
    required this.failedCalls,
    required this.status,
    required this.toolCalls,
  });

  /// The employee whose run it was, for its scratchpad (feature `employee_scratchpad`).
  final String? employeeId;
  final int failedCalls;
  /// `runs.status`: created | starting | running | waiting | cancelling | succeeded |
  /// failed | cancelled | unknown. A string, so a new state never breaks a client.
  final String status;
  final int toolCalls;

  factory RunBrief.fromJson(Map<String, dynamic> json) => RunBrief(
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    failedCalls: (json['failed_calls'] as num).toInt(),
    status: (json['status'] as String),
    toolCalls: (json['tool_calls'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
    'failed_calls': failedCalls,
    'status': status,
    'tool_calls': toolCalls,
  };
}

/// What a board member decided on a run's questions or plan (`MessageView::decision`).
/// `MessageView::text` stays the readable sentence for clients that do not know it.
final class RunDecision {
  const RunDecision({
    this.answers = const [],
    this.feedback,
    required this.kind,
  });

  /// For `answered`: each question with its answer, in the order they were noted.
  final List<QuestionAnswer> answers;
  /// For a plan decision: what the board member wrote with it, if anything.
  final String? feedback;
  /// One of `run_decision`. A string, not an enum, so a new kind never breaks an
  /// older client: it shows `text` instead.
  final String kind;

  factory RunDecision.fromJson(Map<String, dynamic> json) => RunDecision(
    answers: (json['answers'] == null ? const [] : (json['answers'] as List<dynamic>).map((x0) => QuestionAnswer.fromJson(x0 as Map<String, dynamic>)).toList()),
    feedback: (json['feedback'] == null ? null : (json['feedback'] as String)),
    kind: (json['kind'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'answers': answers.map((x0) => x0.toJson()).toList(),
    if (feedback != null) 'feedback': feedback,
    'kind': kind,
  };
}

/// A run's snapshot row (`run_guidelines`, B6): what it had in its system text.
final class RunGuideline {
  const RunGuideline({
    required this.chars,
    required this.guidelineId,
    required this.level,
    required this.runId,
    required this.title,
    required this.version,
  });

  final int chars;
  final String guidelineId;
  final GuidelineLevel level;
  final String runId;
  final String title;
  final int version;

  factory RunGuideline.fromJson(Map<String, dynamic> json) => RunGuideline(
    chars: (json['chars'] as num).toInt(),
    guidelineId: (json['guideline_id'] as String),
    level: GuidelineLevel.fromJson(json['level'] as String),
    runId: (json['run_id'] as String),
    title: (json['title'] as String),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'chars': chars,
    'guideline_id': guidelineId,
    'level': level.toJson(),
    'run_id': runId,
    'title': title,
    'version': version,
  };
}

/// `run.guidelines`: the versions a run had in its system text (B6), in block
/// order. Kept for every run, also without trace mode; empty for a run without
/// guidelines and for runs from before guidelines.
final class RunGuidelines {
  const RunGuidelines({
    required this.guidelines,
    required this.runId,
  });

  final List<RunGuideline> guidelines;
  final String runId;

  factory RunGuidelines.fromJson(Map<String, dynamic> json) => RunGuidelines(
    guidelines: (json['guidelines'] as List<dynamic>).map((x0) => RunGuideline.fromJson(x0 as Map<String, dynamic>)).toList(),
    runId: (json['run_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'guidelines': guidelines.map((x0) => x0.toJson()).toList(),
    'run_id': runId,
  };
}

/// Something running now: since when, and the output tokens so far (thinking,
/// text and tool input; estimated while a message streams).
final class RunProgress {
  const RunProgress({
    this.compacting,
    required this.outputTokens,
    required this.startedAtMs,
  });

  /// The run is compacting the conversation's context before its turn (feature
  /// `compact_status`); absent on older servers, which show it as plain work.
  final bool? compacting;
  final int outputTokens;
  final int startedAtMs;

  factory RunProgress.fromJson(Map<String, dynamic> json) => RunProgress(
    compacting: (json['compacting'] == null ? null : (json['compacting'] as bool)),
    outputTokens: (json['output_tokens'] as num).toInt(),
    startedAtMs: (json['started_at_ms'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (compacting != null) 'compacting': compacting,
    'output_tokens': outputTokens,
    'started_at_ms': startedAtMs,
  };
}

/// Per-field override at one scope for one purpose.
final class RunSettings {
  const RunSettings({
    this.connection,
    this.effort,
    this.model,
    this.persona,
  });

  final Setting? connection;
  /// Reasoning effort (low, medium, high, xhigh, max) where the runtime supports it.
  final Setting2? effort;
  final Setting2? model;
  /// Optional chat persona. Chat answers neutrally by default (no persona).
  final Setting2? persona;

  factory RunSettings.fromJson(Map<String, dynamic> json) => RunSettings(
    connection: (json['connection'] == null ? null : Setting.fromJson(json['connection'] as Map<String, dynamic>)),
    effort: (json['effort'] == null ? null : Setting2.fromJson(json['effort'] as Map<String, dynamic>)),
    model: (json['model'] == null ? null : Setting2.fromJson(json['model'] as Map<String, dynamic>)),
    persona: (json['persona'] == null ? null : Setting2.fromJson(json['persona'] as Map<String, dynamic>)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (connection != null) 'connection': connection?.toJson(),
    if (effort != null) 'effort': effort?.toJson(),
    if (model != null) 'model': model?.toJson(),
    if (persona != null) 'persona': persona?.toJson(),
  };
}

/// Make a task from the schedule right now (also while paused, and even when
/// an earlier one is still open). The next scheduled time does not change.
final class RunTaskScheduleNow {
  const RunTaskScheduleNow({
    required this.scheduleId,
  });

  final String scheduleId;

  factory RunTaskScheduleNow.fromJson(Map<String, dynamic> json) => RunTaskScheduleNow(
    scheduleId: (json['schedule_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'schedule_id': scheduleId,
  };
}

/// `run.get_trace`: what one run sent to the runtime and what each of its model calls cost
/// (feature `run_trace`). Kept only while trace mode is on, for the latest runs.
final class RunTrace {
  const RunTrace({
    required this.args,
    this.conversationId,
    required this.createdAtMs,
    this.effort,
    required this.images,
    required this.mcpServers,
    this.mode,
    required this.model,
    required this.parts,
    required this.profile,
    required this.runId,
    required this.session,
    required this.steps,
    required this.systemText,
    required this.tools,
    this.usage,
    required this.userTurn,
  });

  /// The command line the runtime was started with (secrets left out); empty when it did not say.
  final List<String> args;
  final String? conversationId;
  final int createdAtMs;
  final String? effort;
  final List<TraceImage> images;
  final List<String> mcpServers;
  /// The employee's mode at work (manual | acceptEdits | auto | plan).
  final String? mode;
  final String model;
  /// Sizes of the parts of the system text and the prompt.
  final List<TracePart> parts;
  /// text | web | work: what the runtime's own tools may do.
  final String profile;
  final String runId;
  /// none | new | continue | resume: `continue` goes on in the conversation's session
  /// with only the new messages; `new` sends the whole conversation (a rebuild).
  final String session;
  /// Every model call of the run, in order.
  final List<TraceStep> steps;
  /// The full system text (appended to the runtime's own on every run, also a continued one).
  final String systemText;
  /// The tools the runtime loaded (their definitions are input of every model call).
  final List<String> tools;
  /// The run's own report (none until it ended).
  final TokenUsage? usage;
  /// The prompt as it went in.
  final String userTurn;

  factory RunTrace.fromJson(Map<String, dynamic> json) => RunTrace(
    args: (json['args'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    conversationId: (json['conversation_id'] == null ? null : (json['conversation_id'] as String)),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    effort: (json['effort'] == null ? null : (json['effort'] as String)),
    images: (json['images'] as List<dynamic>).map((x0) => TraceImage.fromJson(x0 as Map<String, dynamic>)).toList(),
    mcpServers: (json['mcp_servers'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    mode: (json['mode'] == null ? null : (json['mode'] as String)),
    model: (json['model'] as String),
    parts: (json['parts'] as List<dynamic>).map((x0) => TracePart.fromJson(x0 as Map<String, dynamic>)).toList(),
    profile: (json['profile'] as String),
    runId: (json['run_id'] as String),
    session: (json['session'] as String),
    steps: (json['steps'] as List<dynamic>).map((x0) => TraceStep.fromJson(x0 as Map<String, dynamic>)).toList(),
    systemText: (json['system_text'] as String),
    tools: (json['tools'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    usage: (json['usage'] == null ? null : TokenUsage.fromJson(json['usage'] as Map<String, dynamic>)),
    userTurn: (json['user_turn'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'args': args,
    if (conversationId != null) 'conversation_id': conversationId,
    'created_at_ms': createdAtMs,
    if (effort != null) 'effort': effort,
    'images': images.map((x0) => x0.toJson()).toList(),
    'mcp_servers': mcpServers,
    if (mode != null) 'mode': mode,
    'model': model,
    'parts': parts.map((x0) => x0.toJson()).toList(),
    'profile': profile,
    'run_id': runId,
    'session': session,
    'steps': steps.map((x0) => x0.toJson()).toList(),
    'system_text': systemText,
    'tools': tools,
    if (usage != null) 'usage': usage?.toJson(),
    'user_turn': userTurn,
  };
}

/// Where an employee's effective model or effort comes from (`employee.run_overview`).
enum RunValueSource {
  /// The company-wide default.
  company('company'),
  /// The default model of the connection.
  connection('connection'),
  /// Chosen for this employee.
  employee('employee'),
  /// Nothing is chosen: the runtime decides (the model's own effort).
  runtime('runtime');

  const RunValueSource(this.wire);

  final String wire;

  static RunValueSource fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown RunValueSource value: $value'),
      );

  String toJson() => wire;
}

/// A message an employee scheduled (`schedule_message`, R7; feature `scheduled_messages`).
final class ScheduledMessageView {
  const ScheduledMessageView({
    this.channelId,
    required this.createdAtMs,
    required this.dueAtMs,
    required this.fromEmployee,
    required this.fromName,
    required this.id,
    this.loginName,
    this.outcome,
    required this.place,
    this.runId,
    required this.state,
    required this.text,
    this.ticketId,
    required this.toEmployee,
    required this.toName,
  });

  /// The channel of the Team chat it goes to.
  final String? channelId;
  final int createdAtMs;
  final int dueAtMs;
  final String fromEmployee;
  final String fromName;
  final String id;
  /// Whose Claude login its run uses: the one of the run that planned it.
  final String? loginName;
  /// Why it did not go out.
  final String? outcome;
  /// Where it goes, in words ("the discussion of ticket #21").
  final String place;
  /// The run it started.
  final String? runId;
  /// scheduled | delivered | cancelled | not_delivered. A string, so a new state never breaks a client.
  final String state;
  final String text;
  final String? ticketId;
  final String toEmployee;
  final String toName;

  factory ScheduledMessageView.fromJson(Map<String, dynamic> json) => ScheduledMessageView(
    channelId: (json['channel_id'] == null ? null : (json['channel_id'] as String)),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    dueAtMs: (json['due_at_ms'] as num).toInt(),
    fromEmployee: (json['from_employee'] as String),
    fromName: (json['from_name'] as String),
    id: (json['id'] as String),
    loginName: (json['login_name'] == null ? null : (json['login_name'] as String)),
    outcome: (json['outcome'] == null ? null : (json['outcome'] as String)),
    place: (json['place'] as String),
    runId: (json['run_id'] == null ? null : (json['run_id'] as String)),
    state: (json['state'] as String),
    text: (json['text'] as String),
    ticketId: (json['ticket_id'] == null ? null : (json['ticket_id'] as String)),
    toEmployee: (json['to_employee'] as String),
    toName: (json['to_name'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (channelId != null) 'channel_id': channelId,
    'created_at_ms': createdAtMs,
    'due_at_ms': dueAtMs,
    'from_employee': fromEmployee,
    'from_name': fromName,
    'id': id,
    if (loginName != null) 'login_name': loginName,
    if (outcome != null) 'outcome': outcome,
    'place': place,
    if (runId != null) 'run_id': runId,
    'state': state,
    'text': text,
    if (ticketId != null) 'ticket_id': ticketId,
    'to_employee': toEmployee,
    'to_name': toName,
  };
}

/// `scheduled_message.list`: what waits, soonest first, then the latest that went out.
final class ScheduledMessagesSnapshot {
  const ScheduledMessagesSnapshot({
    required this.asOfSeq,
    required this.messages,
  });

  final int asOfSeq;
  final List<ScheduledMessageView> messages;

  factory ScheduledMessagesSnapshot.fromJson(Map<String, dynamic> json) => ScheduledMessagesSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    messages: (json['messages'] as List<dynamic>).map((x0) => ScheduledMessageView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'messages': messages.map((x0) => x0.toJson()).toList(),
  };
}

/// `employee.scratchpad`: an employee's own note (C7; feature `employee_scratchpad`).
final class ScratchpadView {
  const ScratchpadView({
    required this.employeeId,
    required this.text,
    this.updatedAtMs,
  });

  final String employeeId;
  final String text;
  /// When it last changed; none when it was never written.
  final int? updatedAtMs;

  factory ScratchpadView.fromJson(Map<String, dynamic> json) => ScratchpadView(
    employeeId: (json['employee_id'] as String),
    text: (json['text'] as String),
    updatedAtMs: (json['updated_at_ms'] == null ? null : (json['updated_at_ms'] as num).toInt()),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'employee_id': employeeId,
    'text': text,
    if (updatedAtMs != null) 'updated_at_ms': updatedAtMs,
  };
}

/// Send a user message in an existing project conversation (protocol 3.1:
/// there are no standalone chats any more, so the conversation is required).
final class SendChatMessage {
  const SendChatMessage({
    this.confirmSecret,
    required this.conversationId,
    this.files = const [],
    this.images = const [],
    this.interrupt,
    required this.text,
  });

  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final String conversationId;
  /// Files of any type that belong to this message (feature `chat_files`): files on
  /// the server, copied into the conversation's work folder as with an employee message.
  final List<String> files;
  /// Images that belong to this message: files on the server (a remote
  /// device's own uploads), kept with the message.
  final List<String> images;
  /// "Send now": stop the conversation's running (and queued) answers in
  /// the same step, so this message is answered right away.
  final bool? interrupt;
  final String text;

  factory SendChatMessage.fromJson(Map<String, dynamic> json) => SendChatMessage(
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    conversationId: (json['conversation_id'] as String),
    files: (json['files'] == null ? const [] : (json['files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    images: (json['images'] == null ? const [] : (json['images'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    interrupt: (json['interrupt'] == null ? null : (json['interrupt'] as bool)),
    text: (json['text'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'conversation_id': conversationId,
    'files': files,
    'images': images,
    if (interrupt != null) 'interrupt': interrupt,
    'text': text,
  };
}

final class SendEmployeeMessage {
  const SendEmployeeMessage({
    this.confirmSecret,
    required this.conversationId,
    required this.employeeId,
    this.files = const [],
    this.images = const [],
    this.interrupt,
    required this.text,
  });

  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  /// The conversation the message goes to: one of its tasks' conversations,
  /// or its persistent chat (`EmployeeSummary::conversation_id`).
  final String conversationId;
  final String employeeId;
  /// Files of any type to copy into the conversation's work folder (under
  /// `.aurelius/uploads/`): host paths, or this device's upload inbox.
  final List<String> files;
  /// Images that belong to this message (see `SendChatMessage::images`).
  final List<String> images;
  /// "Send now" (see `SendChatMessage::interrupt`).
  final bool? interrupt;
  final String text;

  factory SendEmployeeMessage.fromJson(Map<String, dynamic> json) => SendEmployeeMessage(
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    conversationId: (json['conversation_id'] as String),
    employeeId: (json['employee_id'] as String),
    files: (json['files'] == null ? const [] : (json['files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    images: (json['images'] == null ? const [] : (json['images'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    interrupt: (json['interrupt'] == null ? null : (json['interrupt'] as bool)),
    text: (json['text'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'conversation_id': conversationId,
    'employee_id': employeeId,
    'files': files,
    'images': images,
    if (interrupt != null) 'interrupt': interrupt,
    'text': text,
  };
}

/// A message of the user in the group chat (`group_chat.send`).
final class SendGroupMessage {
  const SendGroupMessage({
    this.channel,
    this.confirmSecret,
    this.files = const [],
    this.images = const [],
    required this.text,
    this.threadParent,
  });

  /// The channel (feature `team_channels`): a project's, or none for the general one.
  final String? channel;
  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  /// Files of any type for the employees (host paths, or this device's upload inbox).
  final List<String> files;
  /// Images that belong to this message (see `SendChatMessage::images`).
  final List<String> images;
  final String text;
  /// A reply in the thread of this message of the channel (feature
  /// `chat_threads`): a message of the same channel that is not itself a reply.
  final String? threadParent;

  factory SendGroupMessage.fromJson(Map<String, dynamic> json) => SendGroupMessage(
    channel: (json['channel'] == null ? null : (json['channel'] as String)),
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    files: (json['files'] == null ? const [] : (json['files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    images: (json['images'] == null ? const [] : (json['images'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    text: (json['text'] as String),
    threadParent: (json['thread_parent'] == null ? null : (json['thread_parent'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (channel != null) 'channel': channel,
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'files': files,
    'images': images,
    'text': text,
    if (threadParent != null) 'thread_parent': threadParent,
  };
}

/// A user message in a project conversation; without `conversation_id` a new
/// conversation is made. Messages in the read-only conversations (those of
/// employees from before 1.18) are refused.
final class SendProjectMessage {
  const SendProjectMessage({
    this.agentMode,
    this.browserAllowed,
    this.confirmSecret,
    this.conversationId,
    this.files = const [],
    this.images = const [],
    this.interrupt,
    required this.projectId,
    this.screenAllowed,
    required this.text,
  });

  final AgentMode? agentMode;
  final bool? browserAllowed;
  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final String? conversationId;
  /// Files of any type to copy into the project folder (under
  /// `.aurelius/uploads/`): host paths, or this device's upload inbox.
  final List<String> files;
  /// Images that belong to this message (see `SendChatMessage::images`).
  final List<String> images;
  /// "Send now" (see `SendChatMessage::interrupt`).
  final bool? interrupt;
  final String projectId;
  final bool? screenAllowed;
  final String text;

  factory SendProjectMessage.fromJson(Map<String, dynamic> json) => SendProjectMessage(
    agentMode: (json['agent_mode'] == null ? null : AgentMode.fromJson(json['agent_mode'] as String)),
    browserAllowed: (json['browser_allowed'] == null ? null : (json['browser_allowed'] as bool)),
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    conversationId: (json['conversation_id'] == null ? null : (json['conversation_id'] as String)),
    files: (json['files'] == null ? const [] : (json['files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    images: (json['images'] == null ? const [] : (json['images'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    interrupt: (json['interrupt'] == null ? null : (json['interrupt'] as bool)),
    projectId: (json['project_id'] as String),
    screenAllowed: (json['screen_allowed'] == null ? null : (json['screen_allowed'] as bool)),
    text: (json['text'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (agentMode != null) 'agent_mode': agentMode?.toJson(),
    if (browserAllowed != null) 'browser_allowed': browserAllowed,
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    if (conversationId != null) 'conversation_id': conversationId,
    'files': files,
    'images': images,
    if (interrupt != null) 'interrupt': interrupt,
    'project_id': projectId,
    if (screenAllowed != null) 'screen_allowed': screenAllowed,
    'text': text,
  };
}

/// "Send now" for a user message that still waits (1.9): the answers running
/// or queued ahead of it stop and its own answer starts at once; the messages
/// queued after it keep their place. Refused once that run started.
final class SendQueuedNow {
  const SendQueuedNow({
    required this.conversationId,
    required this.messageId,
  });

  final String conversationId;
  final String messageId;

  factory SendQueuedNow.fromJson(Map<String, dynamic> json) => SendQueuedNow(
    conversationId: (json['conversation_id'] as String),
    messageId: (json['message_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversation_id': conversationId,
    'message_id': messageId,
  };
}

/// Daemon → client.
sealed class ServerMessage {
  const ServerMessage();

  factory ServerMessage.fromJson(Map<String, dynamic> json) {
    final tag = json['type'] as String;
    switch (tag) {
      case 'claude_login_prompt':
        return ServerMessageClaudeLoginPrompt(value: ClaudeLoginPrompt.fromJson(json as Map<String, dynamic>));
      case 'event':
        return ServerMessageEvent(value: Event.fromJson(json as Map<String, dynamic>));
      case 'load_changed':
        return ServerMessageLoadChanged(value: LoadSnapshot.fromJson(json as Map<String, dynamic>));
      case 'power_changed':
        return ServerMessagePowerChanged(value: PowerState.fromJson(json as Map<String, dynamic>));
      case 'response':
        return ServerMessageResponse(
          error: (json['error'] == null ? null : AppError.fromJson(json['error'] as Map<String, dynamic>)),
          ok: (json['ok'] == null ? null : ResponseBody.fromJson(json['ok'] as Map<String, dynamic>)),
          requestId: (json['request_id'] as num).toInt(),
        );
      case 'signed_out':
        return ServerMessageSignedOut(
          reason: SignOutReason.fromJson(json['reason'] as String),
        );
      case 'stream':
        return ServerMessageStream(value: StreamDelta.fromJson(json as Map<String, dynamic>));
      case 'stream_append':
        return ServerMessageStreamAppend(value: StreamAppend.fromJson(json as Map<String, dynamic>));
      default:
        throw FormatException('Unknown ServerMessage type: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

/// What `claude auth login` asks, only to the connection that started the
/// link (`claude_login.link`); never an event, never logged.
final class ServerMessageClaudeLoginPrompt extends ServerMessage {
  const ServerMessageClaudeLoginPrompt({
    required this.value,
  });

  final ClaudeLoginPrompt value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'claude_login_prompt',
      ...value.toJson(),
    };
}

/// A committed event (durable); replayed from the log after `event.subscribe`.
final class ServerMessageEvent extends ServerMessage {
  const ServerMessageEvent({
    required this.value,
  });

  final Event value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'event',
      ...value.toJson(),
    };
}

/// The machine's load or the admission changed. While runs wait, at most every 2 s.
final class ServerMessageLoadChanged extends ServerMessage {
  const ServerMessageLoadChanged({
    required this.value,
  });

  final LoadSnapshot value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'load_changed',
      ...value.toJson(),
    };
}

/// The pending shutdown changed.
final class ServerMessagePowerChanged extends ServerMessage {
  const ServerMessagePowerChanged({
    required this.value,
  });

  final PowerState value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'power_changed',
      ...value.toJson(),
    };
}

final class ServerMessageResponse extends ServerMessage {
  const ServerMessageResponse({
    this.error,
    this.ok,
    required this.requestId,
  });

  final AppError? error;
  final ResponseBody? ok;
  final int requestId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'response',
      if (error != null) 'error': error?.toJson(),
      if (ok != null) 'ok': ok?.toJson(),
      'request_id': requestId,
    };
}

/// The session of this connection ended (`user.logout`, `session.revoke`, a
/// new password, `user.disable`, expiry); the server closes the connection
/// right after. The client shows "You were signed out on this device." and
/// its sign-in screen (a logout of its own needs no message).
final class ServerMessageSignedOut extends ServerMessage {
  const ServerMessageSignedOut({
    required this.reason,
  });

  final SignOutReason reason;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'signed_out',
      'reason': reason.toJson(),
    };
}

/// Ephemeral streamed text; after a reconnect the client refetches instead.
final class ServerMessageStream extends ServerMessage {
  const ServerMessageStream({
    required this.value,
  });

  final StreamDelta value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'stream',
      ...value.toJson(),
    };
}

/// Remote clients only: streamed text as an append.
final class ServerMessageStreamAppend extends ServerMessage {
  const ServerMessageStreamAppend({
    required this.value,
  });

  final StreamAppend value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'stream_append',
      ...value.toJson(),
    };
}

/// `server.get_name`: the name clients show, and where it comes from.
final class ServerNameView {
  const ServerNameView({
    this.chosen,
    required this.computerName,
    required this.name,
    required this.version,
  });

  /// The name a board member chose; none: the computer's name is used.
  final String? chosen;
  /// The computer's name, the default ("Default: ‹computer_name›").
  final String computerName;
  /// What clients show: the chosen name, else the computer's name. The same
  /// as `HelloResult::host_name`.
  final String name;
  /// Counts the changes; 0 before the first one.
  final int version;

  factory ServerNameView.fromJson(Map<String, dynamic> json) => ServerNameView(
    chosen: (json['chosen'] == null ? null : (json['chosen'] as String)),
    computerName: (json['computer_name'] as String),
    name: (json['name'] as String),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (chosen != null) 'chosen': chosen,
    'computer_name': computerName,
    'name': name,
    'version': version,
  };
}

/// Where the dispatcher is with a session (`WorkSessionView::dispatch`).
enum SessionDispatch {
  /// Its job runs: the session waits for its choice.
  choosing('choosing'),
  /// It chose nobody (or could not): the board member chooses.
  nobody('nobody');

  const SessionDispatch(this.wire);

  final String wire;

  static SessionDispatch fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown SessionDispatch value: $value'),
      );

  String toJson() => wire;
}

/// One work session in the History (feature `session_history`): on the day of its
/// last activity, with a summary of what was done in it. Every board member's
/// session is in it; only the one who started it writes in it.
final class SessionHistoryEntry {
  const SessionHistoryEntry({
    this.branch,
    required this.conversationId,
    required this.createdAtMs,
    required this.employeeId,
    required this.employeeName,
    required this.lastActivityMs,
    required this.projectId,
    required this.projectName,
    required this.startedBy,
    this.startedByName,
    this.summary,
    required this.summaryState,
    required this.title,
  });

  /// The session's own branch, when it works in a worktree.
  final String? branch;
  final String conversationId;
  final int createdAtMs;
  final String employeeId;
  final String employeeName;
  /// The newest message (else its start): the day it shows on.
  final int lastActivityMs;
  final String projectId;
  final String projectName;
  /// The board member who started it (the only one who writes in it).
  final String startedBy;
  /// Their name; none when the user is gone.
  final String? startedByName;
  /// What was done in it (through the vault redactor), when `summary_state` is `ready`.
  final String? summary;
  /// `before_history` here: no summary was asked for yet ("No summary yet").
  final HistorySummaryState summaryState;
  final String title;

  factory SessionHistoryEntry.fromJson(Map<String, dynamic> json) => SessionHistoryEntry(
    branch: (json['branch'] == null ? null : (json['branch'] as String)),
    conversationId: (json['conversation_id'] as String),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    employeeId: (json['employee_id'] as String),
    employeeName: (json['employee_name'] as String),
    lastActivityMs: (json['last_activity_ms'] as num).toInt(),
    projectId: (json['project_id'] as String),
    projectName: (json['project_name'] as String),
    startedBy: (json['started_by'] as String),
    startedByName: (json['started_by_name'] == null ? null : (json['started_by_name'] as String)),
    summary: (json['summary'] == null ? null : (json['summary'] as String)),
    summaryState: HistorySummaryState.fromJson(json['summary_state'] as String),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (branch != null) 'branch': branch,
    'conversation_id': conversationId,
    'created_at_ms': createdAtMs,
    'employee_id': employeeId,
    'employee_name': employeeName,
    'last_activity_ms': lastActivityMs,
    'project_id': projectId,
    'project_name': projectName,
    'started_by': startedBy,
    if (startedByName != null) 'started_by_name': startedByName,
    if (summary != null) 'summary': summary,
    'summary_state': summaryState.toJson(),
    'title': title,
  };
}

/// `work_session.history`: sessions, the latest activity first.
final class SessionHistoryPage {
  const SessionHistoryPage({
    required this.asOfSeq,
    required this.entries,
    this.hasMore = false,
  });

  final int asOfSeq;
  final List<SessionHistoryEntry> entries;
  /// There are older rows: ask again with the last row's `last_activity_ms` and `conversation_id`.
  final bool hasMore;

  factory SessionHistoryPage.fromJson(Map<String, dynamic> json) => SessionHistoryPage(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    entries: (json['entries'] as List<dynamic>).map((x0) => SessionHistoryEntry.fromJson(x0 as Map<String, dynamic>)).toList(),
    hasMore: (json['has_more'] == null ? false : (json['has_more'] as bool)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'entries': entries.map((x0) => x0.toJson()).toList(),
    'has_more': hasMore,
  };
}

/// One signed-in device of a board member (`session.list`).
final class SessionView {
  const SessionView({
    this.clientId,
    required this.createdAtMs,
    required this.current,
    required this.deviceName,
    required this.expiresAtMs,
    required this.id,
    required this.lastSeenMs,
    required this.userId,
  });

  /// The paired device; none: the desktop on the server itself (local IPC).
  final String? clientId;
  final int createdAtMs;
  /// The session of the connection that asked.
  final bool current;
  /// What the device called itself at `user.login`.
  final String deviceName;
  /// The absolute end (90 days); 30 days without use end it earlier.
  final int expiresAtMs;
  final String id;
  final int lastSeenMs;
  final String userId;

  factory SessionView.fromJson(Map<String, dynamic> json) => SessionView(
    clientId: (json['client_id'] == null ? null : (json['client_id'] as String)),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    current: (json['current'] as bool),
    deviceName: (json['device_name'] as String),
    expiresAtMs: (json['expires_at_ms'] as num).toInt(),
    id: (json['id'] as String),
    lastSeenMs: (json['last_seen_ms'] as num).toInt(),
    userId: (json['user_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (clientId != null) 'client_id': clientId,
    'created_at_ms': createdAtMs,
    'current': current,
    'device_name': deviceName,
    'expires_at_ms': expiresAtMs,
    'id': id,
    'last_seen_ms': lastSeenMs,
    'user_id': userId,
  };
}

/// `session.list`: newest activity first; never a token or its hash.
final class SessionsSnapshot {
  const SessionsSnapshot({
    required this.sessions,
    required this.userId,
  });

  final List<SessionView> sessions;
  final String userId;

  factory SessionsSnapshot.fromJson(Map<String, dynamic> json) => SessionsSnapshot(
    sessions: (json['sessions'] as List<dynamic>).map((x0) => SessionView.fromJson(x0 as Map<String, dynamic>)).toList(),
    userId: (json['user_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'sessions': sessions.map((x0) => x0.toJson()).toList(),
    'user_id': userId,
  };
}

final class SetAgentMode {
  const SetAgentMode({
    required this.conversationId,
    required this.mode,
  });

  final String conversationId;
  final AgentMode mode;

  factory SetAgentMode.fromJson(Map<String, dynamic> json) => SetAgentMode(
    conversationId: (json['conversation_id'] as String),
    mode: AgentMode.fromJson(json['mode'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversation_id': conversationId,
    'mode': mode.toJson(),
  };
}

/// Allow or forbid the agent to use the user's own browser (Claude in Chrome).
/// The browser shares the user's logins, so this is off until the user turns it on.
final class SetBrowserAllowed {
  const SetBrowserAllowed({
    required this.allowed,
    required this.conversationId,
  });

  final bool allowed;
  final String conversationId;

  factory SetBrowserAllowed.fromJson(Map<String, dynamic> json) => SetBrowserAllowed(
    allowed: (json['allowed'] as bool),
    conversationId: (json['conversation_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'allowed': allowed,
    'conversation_id': conversationId,
  };
}

/// The model and effort of one conversation (a thread with an employee); each
/// left unset follows the employee's own settings.
final class SetConversationRun {
  const SetConversationRun({
    required this.conversationId,
    this.effort,
    this.model,
  });

  final String conversationId;
  final String? effort;
  final String? model;

  factory SetConversationRun.fromJson(Map<String, dynamic> json) => SetConversationRun(
    conversationId: (json['conversation_id'] as String),
    effort: (json['effort'] == null ? null : (json['effort'] as String)),
    model: (json['model'] == null ? null : (json['model'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'conversation_id': conversationId,
    if (effort != null) 'effort': effort,
    if (model != null) 'model': model,
  };
}

/// Who chooses the owner of a started ticket. `employee_id` none: for a project,
/// the company's dispatcher; for the company, no dispatcher (tickets wait in
/// Todo for a board member, B5).
final class SetDispatcher {
  const SetDispatcher({
    this.employeeId,
  });

  final String? employeeId;

  factory SetDispatcher.fromJson(Map<String, dynamic> json) => SetDispatcher(
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
  };
}

/// Give an employee its own workspace folder (its chat works there), or `None`.
final class SetEmployeeFolder {
  const SetEmployeeFolder({
    required this.employeeId,
    this.path,
  });

  final String employeeId;
  final String? path;

  factory SetEmployeeFolder.fromJson(Map<String, dynamic> json) => SetEmployeeFolder(
    employeeId: (json['employee_id'] as String),
    path: (json['path'] == null ? null : (json['path'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'employee_id': employeeId,
    if (path != null) 'path': path,
  };
}

/// Say who an employee reports to, or make it a root (`None`). Anyone in the
/// company who is not archived, never a cycle, at most `org::MAX_CHART_DEPTH` levels.
final class SetEmployeeReportsTo {
  const SetEmployeeReportsTo({
    required this.employeeId,
    this.reportsTo,
  });

  final String employeeId;
  final String? reportsTo;

  factory SetEmployeeReportsTo.fromJson(Map<String, dynamic> json) => SetEmployeeReportsTo(
    employeeId: (json['employee_id'] as String),
    reportsTo: (json['reports_to'] == null ? null : (json['reports_to'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'employee_id': employeeId,
    if (reportsTo != null) 'reports_to': reportsTo,
  };
}

final class SetEmployeeStatus {
  const SetEmployeeStatus({
    required this.employeeId,
    required this.status,
  });

  final String employeeId;
  final LifecycleStatus status;

  factory SetEmployeeStatus.fromJson(Map<String, dynamic> json) => SetEmployeeStatus(
    employeeId: (json['employee_id'] as String),
    status: LifecycleStatus.fromJson(json['status'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'employee_id': employeeId,
    'status': status.toJson(),
  };
}

/// The server setting "Follow the org chart" (on by default): delegation stays
/// in the employee's own subtree, hand-overs go to the boss or to peers, and
/// "Needs you" goes to the boss first.
final class SetFollowOrgChart {
  const SetFollowOrgChart({
    required this.follow,
  });

  final bool follow;

  factory SetFollowOrgChart.fromJson(Map<String, dynamic> json) => SetFollowOrgChart(
    follow: (json['follow'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'follow': follow,
  };
}

final class SetIdeaStatus {
  const SetIdeaStatus({
    required this.ideaId,
    required this.status,
  });

  final String ideaId;
  final IdeaStatus status;

  factory SetIdeaStatus.fromJson(Map<String, dynamic> json) => SetIdeaStatus(
    ideaId: (json['idea_id'] as String),
    status: IdeaStatus.fromJson(json['status'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'idea_id': ideaId,
    'status': status.toJson(),
  };
}

/// `user.set_password`. No password is ever printed.
final class SetPasswordParams {
  const SetPasswordParams({
    required this.newPassword,
    this.oldPassword,
    this.userId,
  });

  final String newPassword;
  /// The current password; required for the own one.
  final String? oldPassword;
  /// Another board member (a temporary password); none: the session's own.
  final String? userId;

  factory SetPasswordParams.fromJson(Map<String, dynamic> json) => SetPasswordParams(
    newPassword: (json['new_password'] as String),
    oldPassword: (json['old_password'] == null ? null : (json['old_password'] as String)),
    userId: (json['user_id'] == null ? null : (json['user_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'new_password': newPassword,
    if (oldPassword != null) 'old_password': oldPassword,
    if (userId != null) 'user_id': userId,
  };
}

/// Change a project's description; an empty (or blank) text clears it.
final class SetProjectDescription {
  const SetProjectDescription({
    required this.description,
    required this.projectId,
  });

  final String description;
  final String projectId;

  factory SetProjectDescription.fromJson(Map<String, dynamic> json) => SetProjectDescription(
    description: (json['description'] as String),
    projectId: (json['project_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'description': description,
    'project_id': projectId,
  };
}

/// Point a project at another folder on disk (its agents may write there).
final class SetProjectFolder {
  const SetProjectFolder({
    required this.path,
    required this.projectId,
  });

  final String path;
  final String projectId;

  factory SetProjectFolder.fromJson(Map<String, dynamic> json) => SetProjectFolder(
    path: (json['path'] as String),
    projectId: (json['project_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'path': path,
    'project_id': projectId,
  };
}

final class SetProjectStatus {
  const SetProjectStatus({
    required this.projectId,
    required this.status,
  });

  final String projectId;
  final LifecycleStatus status;

  factory SetProjectStatus.fromJson(Map<String, dynamic> json) => SetProjectStatus(
    projectId: (json['project_id'] as String),
    status: LifecycleStatus.fromJson(json['status'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'project_id': projectId,
    'status': status.toJson(),
  };
}

final class SetRunDefaults {
  const SetRunDefaults({
    required this.purpose,
    required this.scope,
    required this.settings,
  });

  final Purpose purpose;
  final ConfigScope scope;
  final RunSettings settings;

  factory SetRunDefaults.fromJson(Map<String, dynamic> json) => SetRunDefaults(
    purpose: Purpose.fromJson(json['purpose'] as String),
    scope: ConfigScope.fromJson(json['scope'] as Map<String, dynamic>),
    settings: RunSettings.fromJson(json['settings'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'purpose': purpose.toJson(),
    'scope': scope.toJson(),
    'settings': settings.toJson(),
  };
}

/// Allow the agent to look at the user's screen (screenshots of the desktop or
/// one window) without asking each time. Off by default: the agent may still
/// ask for a screenshot, and the user decides in the inbox.
final class SetScreenAllowed {
  const SetScreenAllowed({
    required this.allowed,
    required this.conversationId,
  });

  final bool allowed;
  final String conversationId;

  factory SetScreenAllowed.fromJson(Map<String, dynamic> json) => SetScreenAllowed(
    allowed: (json['allowed'] as bool),
    conversationId: (json['conversation_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'allowed': allowed,
    'conversation_id': conversationId,
  };
}

/// Pause (`true`) or resume (`false`). Resuming plans the next occurrence
/// from now; what was due while paused is not made up.
final class SetTaskSchedulePaused {
  const SetTaskSchedulePaused({
    required this.paused,
    required this.scheduleId,
  });

  final bool paused;
  final String scheduleId;

  factory SetTaskSchedulePaused.fromJson(Map<String, dynamic> json) => SetTaskSchedulePaused(
    paused: (json['paused'] as bool),
    scheduleId: (json['schedule_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'paused': paused,
    'schedule_id': scheduleId,
  };
}

/// Who runs the full test suite once for a card in Testing (feature
/// `board_testing`). `employee_id` none: no tester, so no Testing step: cards
/// go from Doing to Review as before.
final class SetTester {
  const SetTester({
    this.employeeId,
  });

  final String? employeeId;

  factory SetTester.fromJson(Map<String, dynamic> json) => SetTester(
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
  };
}

/// A field that can inherit, hold a value, or be explicitly disabled.
sealed class Setting {
  const Setting();

  factory Setting.fromJson(Map<String, dynamic> json) {
    final tag = json['mode'] as String;
    switch (tag) {
      case 'disabled':
        return const SettingDisabled();
      case 'inherit':
        return const SettingInherit();
      case 'value':
        return SettingValue(value: (json['value'] as String));
      default:
        throw FormatException('Unknown Setting mode: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

final class SettingDisabled extends Setting {
  const SettingDisabled();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'mode': 'disabled',
    };
}

final class SettingInherit extends Setting {
  const SettingInherit();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'mode': 'inherit',
    };
}

final class SettingValue extends Setting {
  const SettingValue({
    required this.value,
  });

  final String value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'mode': 'value',
      'value': value,
    };
}

/// A field that can inherit, hold a value, or be explicitly disabled.
sealed class Setting2 {
  const Setting2();

  factory Setting2.fromJson(Map<String, dynamic> json) {
    final tag = json['mode'] as String;
    switch (tag) {
      case 'disabled':
        return const Setting2Disabled();
      case 'inherit':
        return const Setting2Inherit();
      case 'value':
        return Setting2Value(value: (json['value'] as String));
      default:
        throw FormatException('Unknown Setting2 mode: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

final class Setting2Disabled extends Setting2 {
  const Setting2Disabled();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'mode': 'disabled',
    };
}

final class Setting2Inherit extends Setting2 {
  const Setting2Inherit();

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'mode': 'inherit',
    };
}

final class Setting2Value extends Setting2 {
  const Setting2Value({
    required this.value,
  });

  final String value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'mode': 'value',
      'value': value,
    };
}

enum ShutdownWhen {
  /// After the grace countdown (60 s).
  now('now'),
  /// Once no run is running or queued and no task is running or queued.
  whenIdle('when_idle');

  const ShutdownWhen(this.wire);

  final String wire;

  static ShutdownWhen fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown ShutdownWhen value: $value'),
      );

  String toJson() => wire;
}

/// Why the server ended a connection's session (`ServerMessage::SignedOut`).
enum SignOutReason {
  /// The board member was disabled.
  disabled('disabled'),
  /// 30 days without use, or 90 days in all.
  expired('expired'),
  /// `user.logout` on this device.
  loggedOut('logged_out'),
  /// The password changed (on another device, or reset by another board member).
  passwordChanged('password_changed'),
  /// `session.revoke` (this or another device), or the paired device was revoked.
  revoked('revoked');

  const SignOutReason(this.wire);

  final String wire;

  static SignOutReason fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown SignOutReason value: $value'),
      );

  String toJson() => wire;
}

final class SourceView {
  const SourceView({
    this.error,
    required this.n,
    this.title,
    required this.url,
  });

  final String? error;
  final int n;
  final String? title;
  final String url;

  factory SourceView.fromJson(Map<String, dynamic> json) => SourceView(
    error: (json['error'] == null ? null : (json['error'] as String)),
    n: (json['n'] as num).toInt(),
    title: (json['title'] == null ? null : (json['title'] as String)),
    url: (json['url'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (error != null) 'error': error,
    'n': n,
    if (title != null) 'title': title,
    'url': url,
  };
}

/// Turn an idea into a task for this employee (its title and note become the
/// task's title and goal); the idea is then done and points at the task.
final class StartIdea {
  const StartIdea({
    required this.employeeId,
    required this.ideaId,
    this.run,
  });

  final String employeeId;
  final String ideaId;
  /// Mode, model and effort of the task; what is left out follows the employee.
  final TaskRun? run;

  factory StartIdea.fromJson(Map<String, dynamic> json) => StartIdea(
    employeeId: (json['employee_id'] as String),
    ideaId: (json['idea_id'] as String),
    run: (json['run'] == null ? null : TaskRun.fromJson(json['run'] as Map<String, dynamic>)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'employee_id': employeeId,
    'idea_id': ideaId,
    if (run != null) 'run': run?.toJson(),
  };
}

/// One entry of the catalogue in the repo (`employee.start_profiles`, C6).
final class StartProfile {
  const StartProfile({
    this.accessLevel,
    required this.description,
    this.hired = const [],
    required this.instructions,
    required this.key,
    required this.label,
    required this.name,
    required this.role,
    required this.summary,
    this.title,
    this.toolPolicy,
  });

  final AccessLevel? accessLevel;
  final String description;
  /// The employees (not archived) that started from this profile.
  final List<String> hired;
  final String instructions;
  /// Stable key for `employee.create` (`engineer`, `chief_of_staff`, ...).
  final String key;
  final String label;
  /// A suggested name; another employee may already have it.
  final String name;
  final String role;
  /// One sentence for the picker.
  final String summary;
  final String? title;
  final ToolPolicy? toolPolicy;

  factory StartProfile.fromJson(Map<String, dynamic> json) => StartProfile(
    accessLevel: (json['access_level'] == null ? null : AccessLevel.fromJson(json['access_level'] as String)),
    description: (json['description'] as String),
    hired: (json['hired'] == null ? const [] : (json['hired'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    instructions: (json['instructions'] as String),
    key: (json['key'] as String),
    label: (json['label'] as String),
    name: (json['name'] as String),
    role: (json['role'] as String),
    summary: (json['summary'] as String),
    title: (json['title'] == null ? null : (json['title'] as String)),
    toolPolicy: (json['tool_policy'] == null ? null : ToolPolicy.fromJson(json['tool_policy'] as Map<String, dynamic>)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (accessLevel != null) 'access_level': accessLevel?.toJson(),
    'description': description,
    'hired': hired,
    'instructions': instructions,
    'key': key,
    'label': label,
    'name': name,
    'role': role,
    'summary': summary,
    if (title != null) 'title': title,
    if (toolPolicy != null) 'tool_policy': toolPolicy?.toJson(),
  };
}

/// `employee.start_profiles` (feature `employee_profiles`).
final class StartProfiles {
  const StartProfiles({
    required this.profiles,
  });

  final List<StartProfile> profiles;

  factory StartProfiles.fromJson(Map<String, dynamic> json) => StartProfiles(
    profiles: (json['profiles'] as List<dynamic>).map((x0) => StartProfile.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'profiles': profiles.map((x0) => x0.toJson()).toList(),
  };
}

/// A new, empty project conversation. It is named after its first message.
final class StartProjectConversation {
  const StartProjectConversation({
    this.agentMode,
    this.browserAllowed,
    required this.projectId,
    this.screenAllowed,
  });

  final AgentMode? agentMode;
  final bool? browserAllowed;
  final String projectId;
  final bool? screenAllowed;

  factory StartProjectConversation.fromJson(Map<String, dynamic> json) => StartProjectConversation(
    agentMode: (json['agent_mode'] == null ? null : AgentMode.fromJson(json['agent_mode'] as String)),
    browserAllowed: (json['browser_allowed'] == null ? null : (json['browser_allowed'] as bool)),
    projectId: (json['project_id'] as String),
    screenAllowed: (json['screen_allowed'] == null ? null : (json['screen_allowed'] as bool)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (agentMode != null) 'agent_mode': agentMode?.toJson(),
    if (browserAllowed != null) 'browser_allowed': browserAllowed,
    'project_id': projectId,
    if (screenAllowed != null) 'screen_allowed': screenAllowed,
  };
}

/// Start a work session with its first message.
final class StartWorkSession {
  const StartWorkSession({
    this.confirmSecret,
    required this.employeeId,
    this.files = const [],
    this.images = const [],
    required this.projectId,
    this.run,
    required this.text,
    this.worktree,
  });

  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final String employeeId;
  /// Files that belong to the first message (feature `chat_files`, see
  /// `SendChatMessage::files`). Not with `worktree`: that folder does not exist yet.
  final List<String> files;
  /// Images that belong to the first message (see `SendChatMessage::images`).
  final List<String> images;
  final String projectId;
  /// Mode, model and effort it starts with (empty: the employee's defaults).
  final TaskRun? run;
  final String text;
  /// Work in a worktree of its own instead of the project's folder. Needs a
  /// project folder that is a git repository with a main branch.
  final bool? worktree;

  factory StartWorkSession.fromJson(Map<String, dynamic> json) => StartWorkSession(
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    employeeId: (json['employee_id'] as String),
    files: (json['files'] == null ? const [] : (json['files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    images: (json['images'] == null ? const [] : (json['images'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    projectId: (json['project_id'] as String),
    run: (json['run'] == null ? null : TaskRun.fromJson(json['run'] as Map<String, dynamic>)),
    text: (json['text'] as String),
    worktree: (json['worktree'] == null ? null : (json['worktree'] as bool)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'employee_id': employeeId,
    'files': files,
    'images': images,
    'project_id': projectId,
    if (run != null) 'run': run?.toJson(),
    'text': text,
    if (worktree != null) 'worktree': worktree,
  };
}

/// `guideline.starter_set`.
final class StarterSet {
  const StarterSet({
    required this.templates,
  });

  final List<StarterTemplate> templates;

  factory StarterSet.fromJson(Map<String, dynamic> json) => StarterSet(
    templates: (json['templates'] as List<dynamic>).map((x0) => StarterTemplate.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'templates': templates.map((x0) => x0.toJson()).toList(),
  };
}

/// One template of the starter set (B7, `guideline.starter_set`).
final class StarterTemplate {
  const StarterTemplate({
    required this.chars,
    this.inLibrary = false,
    required this.key,
    required this.level,
    required this.text,
    required this.title,
  });

  final int chars;
  /// A non-archived guideline with this title is in the library already.
  final bool inLibrary;
  /// Stable key for `guideline.add_starter_set` (`rust_style`, …).
  final String key;
  /// What it is meant for: employees (how the team works) or a project (one repository).
  final GuidelineLevel level;
  final String text;
  final String title;

  factory StarterTemplate.fromJson(Map<String, dynamic> json) => StarterTemplate(
    chars: (json['chars'] as num).toInt(),
    inLibrary: (json['in_library'] == null ? false : (json['in_library'] as bool)),
    key: (json['key'] as String),
    level: GuidelineLevel.fromJson(json['level'] as String),
    text: (json['text'] as String),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'chars': chars,
    'in_library': inLibrary,
    'key': key,
    'level': level.toJson(),
    'text': text,
    'title': title,
  };
}

/// Stop one run. Never affects other runs.
final class StopRun {
  const StopRun({
    required this.runId,
  });

  final String runId;

  factory StopRun.fromJson(Map<String, dynamic> json) => StopRun(
    runId: (json['run_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'run_id': runId,
  };
}

/// Ephemeral streamed text for remote clients: `text` continues the message
/// at character `offset`. `offset == 0` carries the full text so far.
final class StreamAppend {
  const StreamAppend({
    this.activity,
    this.activityChanges,
    required this.conversationId,
    this.draft,
    required this.messageId,
    this.mode,
    required this.offset,
    this.progress,
    required this.runId,
    required this.text,
    this.threadParent,
  });

  /// The tools used so far, sent only when they changed since the last
  /// append for this message (offsets are UTF-8 bytes, as in `MessageView`).
  final List<ActivityView>? activity;
  /// `activity` holds only new or changed calls (see `StreamDelta::activity_changes`).
  final bool? activityChanges;
  final String conversationId;
  /// A design being written (see `StreamDelta::draft`).
  final ArtifactDraft? draft;
  final String messageId;
  /// A group chat answer: the mode its run started in (see `StreamDelta::mode`).
  final AgentMode? mode;
  final int offset;
  /// How long it runs and the tokens so far (see `StreamDelta`).
  final RunProgress? progress;
  final String runId;
  final String text;
  /// An answer streaming into a thread: its root (see `StreamDelta::thread_parent`).
  final String? threadParent;

  factory StreamAppend.fromJson(Map<String, dynamic> json) => StreamAppend(
    activity: (json['activity'] == null ? null : (json['activity'] as List<dynamic>).map((x0) => ActivityView.fromJson(x0 as Map<String, dynamic>)).toList()),
    activityChanges: (json['activity_changes'] == null ? null : (json['activity_changes'] as bool)),
    conversationId: (json['conversation_id'] as String),
    draft: (json['draft'] == null ? null : ArtifactDraft.fromJson(json['draft'] as Map<String, dynamic>)),
    messageId: (json['message_id'] as String),
    mode: (json['mode'] == null ? null : AgentMode.fromJson(json['mode'] as String)),
    offset: (json['offset'] as num).toInt(),
    progress: (json['progress'] == null ? null : RunProgress.fromJson(json['progress'] as Map<String, dynamic>)),
    runId: (json['run_id'] as String),
    text: (json['text'] as String),
    threadParent: (json['thread_parent'] == null ? null : (json['thread_parent'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (activity != null) 'activity': activity?.map((x0) => x0.toJson()).toList(),
    if (activityChanges != null) 'activity_changes': activityChanges,
    'conversation_id': conversationId,
    if (draft != null) 'draft': draft?.toJson(),
    'message_id': messageId,
    if (mode != null) 'mode': mode?.toJson(),
    'offset': offset,
    if (progress != null) 'progress': progress?.toJson(),
    'run_id': runId,
    'text': text,
    if (threadParent != null) 'thread_parent': threadParent,
  };
}

/// Ephemeral streamed text. Not domain truth: after reconnect the client
/// refetches messages instead of relying on missed deltas.
final class StreamDelta {
  const StreamDelta({
    required this.activity,
    this.activityChanges,
    required this.conversationId,
    this.draft,
    required this.messageId,
    this.mode,
    this.progress,
    required this.runId,
    required this.text,
    this.threadParent,
  });

  /// All tool use so far (idempotent too), each call shortened (feature
  /// `activity_preview`). With `activity_changes`: only the calls new or
  /// changed since this session's previous update for the message.
  final List<ActivityView> activity;
  /// `activity` holds changes only, to merge by `call_id` (new calls go at
  /// the end); `false` means the full list, as always. Only a session whose
  /// hello `accepts` `activity_changes` gets `true`, and never on the first
  /// update of a message in that session (that one carries every call).
  final bool? activityChanges;
  final String conversationId;
  /// A design the answer is writing now (feature `artifact_drafts`): only on the
  /// update that brings it, and only to a connection that accepts
  /// `artifact_drafts`. Left out elsewhere and by older servers.
  final ArtifactDraft? draft;
  final String messageId;
  /// A group chat answer streaming: the mode its run started in. Left out
  /// elsewhere and by older servers.
  final AgentMode? mode;
  /// How long it runs and how many tokens it wrote so far.
  final RunProgress? progress;
  final String runId;
  /// Full text so far (idempotent; a dropped delta loses nothing).
  final String text;
  /// An employee's answer streaming into a thread (feature `chat_threads`):
  /// the thread's root message in `conversation_id`. Only connections that
  /// accept `chat_threads` get such an update. Left out elsewhere and by older servers.
  final String? threadParent;

  factory StreamDelta.fromJson(Map<String, dynamic> json) => StreamDelta(
    activity: (json['activity'] as List<dynamic>).map((x0) => ActivityView.fromJson(x0 as Map<String, dynamic>)).toList(),
    activityChanges: (json['activity_changes'] == null ? null : (json['activity_changes'] as bool)),
    conversationId: (json['conversation_id'] as String),
    draft: (json['draft'] == null ? null : ArtifactDraft.fromJson(json['draft'] as Map<String, dynamic>)),
    messageId: (json['message_id'] as String),
    mode: (json['mode'] == null ? null : AgentMode.fromJson(json['mode'] as String)),
    progress: (json['progress'] == null ? null : RunProgress.fromJson(json['progress'] as Map<String, dynamic>)),
    runId: (json['run_id'] as String),
    text: (json['text'] as String),
    threadParent: (json['thread_parent'] == null ? null : (json['thread_parent'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'activity': activity.map((x0) => x0.toJson()).toList(),
    if (activityChanges != null) 'activity_changes': activityChanges,
    'conversation_id': conversationId,
    if (draft != null) 'draft': draft?.toJson(),
    'message_id': messageId,
    if (mode != null) 'mode': mode?.toJson(),
    if (progress != null) 'progress': progress?.toJson(),
    'run_id': runId,
    'text': text,
    if (threadParent != null) 'thread_parent': threadParent,
  };
}

final class SubscriptionUsage {
  const SubscriptionUsage({
    this.error,
    required this.fetchedAtMs,
    required this.limits,
    this.loginUser,
    this.plan,
    required this.raw,
  });

  /// Why there is no data (no Claude Code connection, not signed in, …).
  final String? error;
  final int fetchedAtMs;
  final List<PlanLimitView> limits;
  /// Whose Claude login this is (protocol 5: `subscription.get` answers the
  /// session's own; nobody sees another's usage, B12). None: the server's
  /// own login, until runs move to the board members' logins.
  final String? loginUser;
  /// e.g. "max"; None when unknown.
  final String? plan;
  /// The full `/usage` text (shown when parsing found nothing).
  final String raw;

  factory SubscriptionUsage.fromJson(Map<String, dynamic> json) => SubscriptionUsage(
    error: (json['error'] == null ? null : (json['error'] as String)),
    fetchedAtMs: (json['fetched_at_ms'] as num).toInt(),
    limits: (json['limits'] as List<dynamic>).map((x0) => PlanLimitView.fromJson(x0 as Map<String, dynamic>)).toList(),
    loginUser: (json['login_user'] == null ? null : (json['login_user'] as String)),
    plan: (json['plan'] == null ? null : (json['plan'] as String)),
    raw: (json['raw'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (error != null) 'error': error,
    'fetched_at_ms': fetchedAtMs,
    'limits': limits.map((x0) => x0.toJson()).toList(),
    if (loginUser != null) 'login_user': loginUser,
    if (plan != null) 'plan': plan,
    'raw': raw,
  };
}

final class SubtaskUsage {
  const SubtaskUsage({
    this.employeeName,
    required this.taskId,
    required this.title,
    required this.usage,
  });

  /// Who works on it.
  final String? employeeName;
  final String taskId;
  final String title;
  final TokenUsage usage;

  factory SubtaskUsage.fromJson(Map<String, dynamic> json) => SubtaskUsage(
    employeeName: (json['employee_name'] == null ? null : (json['employee_name'] as String)),
    taskId: (json['task_id'] as String),
    title: (json['title'] as String),
    usage: TokenUsage.fromJson(json['usage'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (employeeName != null) 'employee_name': employeeName,
    'task_id': taskId,
    'title': title,
    'usage': usage.toJson(),
  };
}

/// A system line (`MessageView::system`). Every field but `kind` is optional and
/// only set where `SystemLineKind` names it.
final class SystemLine {
  const SystemLine({
    this.action,
    this.by,
    this.channelId,
    this.conversationId,
    this.from,
    required this.kind,
    this.messageId,
    this.taskId,
    this.ticketId,
    this.to,
  });

  /// What exactly happened within `kind` (feature `ticket_channels`), one of
  /// `system_action`: e.g. `opened` or `closed` on a ticket's line in General,
  /// `approved` on a `plan` line. A client that knows none of them shows `text`,
  /// which stays the readable sentence. A string, not an enum, so a new action
  /// never breaks an older client. Left out by older servers.
  final String? action;
  /// Who did it: a board member's name, an employee's, or "Aurelius".
  final String? by;
  /// The ticket's channel (its discussion) the line links to (feature
  /// `ticket_channels`), e.g. on the line in General when a ticket opens or closes.
  final String? channelId;
  final String? conversationId;
  final String? from;
  final SystemLineKind kind;
  final String? messageId;
  final String? taskId;
  final String? ticketId;
  final String? to;

  factory SystemLine.fromJson(Map<String, dynamic> json) => SystemLine(
    action: (json['action'] == null ? null : (json['action'] as String)),
    by: (json['by'] == null ? null : (json['by'] as String)),
    channelId: (json['channel_id'] == null ? null : (json['channel_id'] as String)),
    conversationId: (json['conversation_id'] == null ? null : (json['conversation_id'] as String)),
    from: (json['from'] == null ? null : (json['from'] as String)),
    kind: SystemLineKind.fromJson(json['kind'] as String),
    messageId: (json['message_id'] == null ? null : (json['message_id'] as String)),
    taskId: (json['task_id'] == null ? null : (json['task_id'] as String)),
    ticketId: (json['ticket_id'] == null ? null : (json['ticket_id'] as String)),
    to: (json['to'] == null ? null : (json['to'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (action != null) 'action': action,
    if (by != null) 'by': by,
    if (channelId != null) 'channel_id': channelId,
    if (conversationId != null) 'conversation_id': conversationId,
    if (from != null) 'from': from,
    'kind': kind.toJson(),
    if (messageId != null) 'message_id': messageId,
    if (taskId != null) 'task_id': taskId,
    if (ticketId != null) 'ticket_id': ticketId,
    if (to != null) 'to': to,
  };
}

/// What a system line in a discussion says happened (`MessageView::system`).
enum SystemLineKind {
  /// The ticket was made from the message `message_id` in `conversation_id`.
  createdFrom('created_from'),
  /// `from` gave the task `task_id` to `to`.
  delegated('delegated'),
  /// `by` put the ticket on Done.
  done('done'),
  /// A mention woke nobody: a loop limit was reached (`to`: who was not woken).
  limit('limit'),
  /// `by` moved the ticket from the column `from` to `to`.
  moved('moved'),
  /// The owner's plan for `task_id` waits for a board member (approve or send back).
  plan('plan'),
  /// The task `task_id` of `from` reported (or failed: the text says so).
  reported('reported'),
  /// The ticket is in Review: the report of `task_id` waits for a board member.
  review('review'),
  /// The summary of the ticket went to the project's memory.
  summary('summary'),
  /// The summary failed (the text says why); a board member can try again (`ticket.retry_summary`).
  summaryFailed('summary_failed'),
  /// In a chat: the ticket `ticket_id` was made from this conversation's message `message_id`.
  ticketCreated('ticket_created');

  const SystemLineKind(this.wire);

  final String wire;

  static SystemLineKind fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown SystemLineKind value: $value'),
      );

  String toJson() => wire;
}

final class TailscaleStatus {
  const TailscaleStatus({
    this.dnsName,
    this.error,
    required this.installed,
    required this.online,
    required this.serving,
  });

  /// MagicDNS name of this PC, without the trailing dot.
  final String? dnsName;
  final String? error;
  final bool installed;
  final bool online;
  /// Tailscale Serve forwards the remote port to the gateway.
  final bool serving;

  factory TailscaleStatus.fromJson(Map<String, dynamic> json) => TailscaleStatus(
    dnsName: (json['dns_name'] == null ? null : (json['dns_name'] as String)),
    error: (json['error'] == null ? null : (json['error'] as String)),
    installed: (json['installed'] as bool),
    online: (json['online'] as bool),
    serving: (json['serving'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (dnsName != null) 'dns_name': dnsName,
    if (error != null) 'error': error,
    'installed': installed,
    'online': online,
    'serving': serving,
  };
}

/// One hand-over of a task to a colleague who suits it better.
final class TaskHandoff {
  const TaskHandoff({
    required this.from,
    this.fromId,
    required this.reason,
    required this.to,
    this.toId,
  });

  final String from;
  final String? fromId;
  final String reason;
  final String to;
  final String? toId;

  factory TaskHandoff.fromJson(Map<String, dynamic> json) => TaskHandoff(
    from: (json['from'] as String),
    fromId: (json['from_id'] == null ? null : (json['from_id'] as String)),
    reason: (json['reason'] as String),
    to: (json['to'] as String),
    toId: (json['to_id'] == null ? null : (json['to_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'from': from,
    if (fromId != null) 'from_id': fromId,
    'reason': reason,
    'to': to,
    if (toId != null) 'to_id': toId,
  };
}

/// Which tasks `task.list` gives by state.
enum TaskListState {
  /// Succeeded, failed or cancelled.
  ended('ended'),
  /// Waiting for the user (input, an approval) or in review.
  needsYou('needs_you'),
  /// Not ended: draft, queued, running, waiting, review.
  open('open');

  const TaskListState(this.wire);

  final String wire;

  static TaskListState fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown TaskListState value: $value'),
      );

  String toJson() => wire;
}

/// The mode, connection, model and effort a task's conversation runs with.
/// Each field left out follows the employee's own settings.
final class TaskRun {
  const TaskRun({
    this.connectionId,
    this.effort,
    this.mode,
    this.model,
  });

  final String? connectionId;
  final String? effort;
  final AgentMode? mode;
  final String? model;

  factory TaskRun.fromJson(Map<String, dynamic> json) => TaskRun(
    connectionId: (json['connection_id'] == null ? null : (json['connection_id'] as String)),
    effort: (json['effort'] == null ? null : (json['effort'] as String)),
    mode: (json['mode'] == null ? null : AgentMode.fromJson(json['mode'] as String)),
    model: (json['model'] == null ? null : (json['model'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (connectionId != null) 'connection_id': connectionId,
    if (effort != null) 'effort': effort,
    if (mode != null) 'mode': mode?.toJson(),
    if (model != null) 'model': model,
  };
}

final class TaskScheduleView {
  const TaskScheduleView({
    required this.attachments,
    required this.createdAtMs,
    required this.criteria,
    required this.employeeId,
    required this.employeeName,
    required this.goal,
    required this.id,
    this.lastRunMs,
    this.lastSkippedMs,
    this.lastTaskId,
    this.nextRunMs,
    this.openTaskId,
    required this.paused,
    required this.projectId,
    required this.recurrence,
    required this.run,
    required this.title,
  });

  /// Copies in the work folder, attached to every task made from it.
  final List<MessageFileView> attachments;
  final int createdAtMs;
  final List<String> criteria;
  final String employeeId;
  final String employeeName;
  final String goal;
  final String id;
  /// When it last made a task, and which one.
  final int? lastRunMs;
  /// When a due run was skipped because the previous task was still open.
  final int? lastSkippedMs;
  final String? lastTaskId;
  /// When it is due next; none while paused.
  final int? nextRunMs;
  /// The task from this schedule that is still open, if any.
  final String? openTaskId;
  final bool paused;
  final String projectId;
  final Recurrence recurrence;
  final TaskRun run;
  final String title;

  factory TaskScheduleView.fromJson(Map<String, dynamic> json) => TaskScheduleView(
    attachments: (json['attachments'] as List<dynamic>).map((x0) => MessageFileView.fromJson(x0 as Map<String, dynamic>)).toList(),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    criteria: (json['criteria'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    employeeId: (json['employee_id'] as String),
    employeeName: (json['employee_name'] as String),
    goal: (json['goal'] as String),
    id: (json['id'] as String),
    lastRunMs: (json['last_run_ms'] == null ? null : (json['last_run_ms'] as num).toInt()),
    lastSkippedMs: (json['last_skipped_ms'] == null ? null : (json['last_skipped_ms'] as num).toInt()),
    lastTaskId: (json['last_task_id'] == null ? null : (json['last_task_id'] as String)),
    nextRunMs: (json['next_run_ms'] == null ? null : (json['next_run_ms'] as num).toInt()),
    openTaskId: (json['open_task_id'] == null ? null : (json['open_task_id'] as String)),
    paused: (json['paused'] as bool),
    projectId: (json['project_id'] as String),
    recurrence: Recurrence.fromJson(json['recurrence'] as Map<String, dynamic>),
    run: TaskRun.fromJson(json['run'] as Map<String, dynamic>),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'attachments': attachments.map((x0) => x0.toJson()).toList(),
    'created_at_ms': createdAtMs,
    'criteria': criteria,
    'employee_id': employeeId,
    'employee_name': employeeName,
    'goal': goal,
    'id': id,
    if (lastRunMs != null) 'last_run_ms': lastRunMs,
    if (lastSkippedMs != null) 'last_skipped_ms': lastSkippedMs,
    if (lastTaskId != null) 'last_task_id': lastTaskId,
    if (nextRunMs != null) 'next_run_ms': nextRunMs,
    if (openTaskId != null) 'open_task_id': openTaskId,
    'paused': paused,
    'project_id': projectId,
    'recurrence': recurrence.toJson(),
    'run': run.toJson(),
    'title': title,
  };
}

/// Recurring tasks. Times are epoch milliseconds; the recurrence is
/// evaluated in the server's local time zone.
final class TaskSchedulesSnapshot {
  const TaskSchedulesSnapshot({
    required this.schedules,
  });

  final List<TaskScheduleView> schedules;

  factory TaskSchedulesSnapshot.fromJson(Map<String, dynamic> json) => TaskSchedulesSnapshot(
    schedules: (json['schedules'] as List<dynamic>).map((x0) => TaskScheduleView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'schedules': schedules.map((x0) => x0.toJson()).toList(),
  };
}

enum TaskStatus {
  cancelled('cancelled'),
  draft('draft'),
  failed('failed'),
  queued('queued'),
  review('review'),
  running('running'),
  succeeded('succeeded'),
  waiting('waiting');

  const TaskStatus(this.wire);

  final String wire;

  static TaskStatus fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown TaskStatus value: $value'),
      );

  String toJson() => wire;
}

final class TaskView {
  const TaskView({
    required this.attachments,
    required this.children,
    this.conversationId,
    required this.createdAtMs,
    required this.criteria,
    this.delegatedBy,
    this.employeeId,
    this.employeeName,
    required this.goal,
    required this.handoffs,
    required this.id,
    this.liveRun,
    this.parentTaskId,
    required this.pendingApprovals,
    required this.projectId,
    this.result,
    required this.run,
    this.scheduleId,
    this.startsAtMs,
    required this.status,
    this.ticketId,
    this.ticketNumber,
    required this.title,
    required this.updatedAtMs,
    this.waitingReason,
    this.waitingUntilMs,
  });

  /// Files attached when the task was made, in the work folder's uploads
  /// area. Left out when there are none.
  final List<MessageFileView> attachments;
  /// Sub-tasks this task's employee delegated to colleagues.
  final List<String> children;
  /// The conversation of the task with its current employee (the task is its
  /// context; plan, findings and results come back there).
  final String? conversationId;
  final int createdAtMs;
  final List<String> criteria;
  final String? delegatedBy;
  final String? employeeId;
  final String? employeeName;
  final String goal;
  /// Hand-overs between employees, oldest first.
  final List<TaskHandoff> handoffs;
  final String id;
  /// A finished task (succeeded, failed or cancelled) in whose conversation a
  /// run goes on or waits to start, after the user (or a manager) wrote to it.
  /// The status does not change: clients show such a task as working or
  /// queued, not as done. A task in Review has it too: Review only says the
  /// report is in, this says the employee is answering a message about it.
  /// Derived, never stored; left out when there is none and for other tasks
  /// that are not finished (their `status` says it). Older servers leave it out.
  final LiveRun? liveRun;
  /// The task this one was delegated from (a sub-task), and its employee.
  final String? parentTaskId;
  /// Actions (and questions) of this task waiting for the user.
  final int pendingApprovals;
  final String projectId;
  final String? result;
  /// Mode, connection, model and effort chosen for the task (unset: the employee's).
  final TaskRun run;
  /// The recurring task this one was made from.
  final String? scheduleId;
  /// A `scheduled` queued task: when it may start (none when not known).
  final int? startsAtMs;
  final TaskStatus status;
  /// The ticket this task works for (a main task or a delegation below it), and its number.
  final String? ticketId;
  final int? ticketNumber;
  final String title;
  final int updatedAtMs;
  /// Why a waiting task waits. A queued task that does not start now has one
  /// too (derived, never stored): `in_line` (its employee works on one task at
  /// a time and another goes first) or `scheduled` (its start is planned for
  /// later, see `starts_at_ms`). Such a task is not at work: leave it out of
  /// active and at-work counts. Older servers leave it out for queued tasks.
  final String? waitingReason;
  /// While it waits for the usage limit ("quota"): when its run goes again.
  final int? waitingUntilMs;

  factory TaskView.fromJson(Map<String, dynamic> json) => TaskView(
    attachments: (json['attachments'] as List<dynamic>).map((x0) => MessageFileView.fromJson(x0 as Map<String, dynamic>)).toList(),
    children: (json['children'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    conversationId: (json['conversation_id'] == null ? null : (json['conversation_id'] as String)),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    criteria: (json['criteria'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    delegatedBy: (json['delegated_by'] == null ? null : (json['delegated_by'] as String)),
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    employeeName: (json['employee_name'] == null ? null : (json['employee_name'] as String)),
    goal: (json['goal'] as String),
    handoffs: (json['handoffs'] as List<dynamic>).map((x0) => TaskHandoff.fromJson(x0 as Map<String, dynamic>)).toList(),
    id: (json['id'] as String),
    liveRun: (json['live_run'] == null ? null : LiveRun.fromJson(json['live_run'] as String)),
    parentTaskId: (json['parent_task_id'] == null ? null : (json['parent_task_id'] as String)),
    pendingApprovals: (json['pending_approvals'] as num).toInt(),
    projectId: (json['project_id'] as String),
    result: (json['result'] == null ? null : (json['result'] as String)),
    run: TaskRun.fromJson(json['run'] as Map<String, dynamic>),
    scheduleId: (json['schedule_id'] == null ? null : (json['schedule_id'] as String)),
    startsAtMs: (json['starts_at_ms'] == null ? null : (json['starts_at_ms'] as num).toInt()),
    status: TaskStatus.fromJson(json['status'] as String),
    ticketId: (json['ticket_id'] == null ? null : (json['ticket_id'] as String)),
    ticketNumber: (json['ticket_number'] == null ? null : (json['ticket_number'] as num).toInt()),
    title: (json['title'] as String),
    updatedAtMs: (json['updated_at_ms'] as num).toInt(),
    waitingReason: (json['waiting_reason'] == null ? null : (json['waiting_reason'] as String)),
    waitingUntilMs: (json['waiting_until_ms'] == null ? null : (json['waiting_until_ms'] as num).toInt()),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'attachments': attachments.map((x0) => x0.toJson()).toList(),
    'children': children,
    if (conversationId != null) 'conversation_id': conversationId,
    'created_at_ms': createdAtMs,
    'criteria': criteria,
    if (delegatedBy != null) 'delegated_by': delegatedBy,
    if (employeeId != null) 'employee_id': employeeId,
    if (employeeName != null) 'employee_name': employeeName,
    'goal': goal,
    'handoffs': handoffs.map((x0) => x0.toJson()).toList(),
    'id': id,
    if (liveRun != null) 'live_run': liveRun?.toJson(),
    if (parentTaskId != null) 'parent_task_id': parentTaskId,
    'pending_approvals': pendingApprovals,
    'project_id': projectId,
    if (result != null) 'result': result,
    'run': run.toJson(),
    if (scheduleId != null) 'schedule_id': scheduleId,
    if (startsAtMs != null) 'starts_at_ms': startsAtMs,
    'status': status.toJson(),
    if (ticketId != null) 'ticket_id': ticketId,
    if (ticketNumber != null) 'ticket_number': ticketNumber,
    'title': title,
    'updated_at_ms': updatedAtMs,
    if (waitingReason != null) 'waiting_reason': waitingReason,
    if (waitingUntilMs != null) 'waiting_until_ms': waitingUntilMs,
  };
}

final class TasksSnapshot {
  const TasksSnapshot({
    required this.asOfSeq,
    required this.tasks,
  });

  final int asOfSeq;
  final List<TaskView> tasks;

  factory TasksSnapshot.fromJson(Map<String, dynamic> json) => TasksSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    tasks: (json['tasks'] as List<dynamic>).map((x0) => TaskView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'tasks': tasks.map((x0) => x0.toJson()).toList(),
  };
}

/// Where a thread is, as `activity_log.list` and `employee.queue` name it (R8, U6).
final class ThreadPlace {
  const ThreadPlace({
    this.channelId,
    this.chatId,
    required this.label,
    this.ticketId,
  });

  final String? channelId;
  /// A board member's chat with an employee, when it is the reader's.
  final String? chatId;
  /// "#21 Fix login", a channel's name, "Chat with sam", or what is left of it.
  final String label;
  final String? ticketId;

  factory ThreadPlace.fromJson(Map<String, dynamic> json) => ThreadPlace(
    channelId: (json['channel_id'] == null ? null : (json['channel_id'] as String)),
    chatId: (json['chat_id'] == null ? null : (json['chat_id'] as String)),
    label: (json['label'] as String),
    ticketId: (json['ticket_id'] == null ? null : (json['ticket_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (channelId != null) 'channel_id': channelId,
    if (chatId != null) 'chat_id': chatId,
    'label': label,
    if (ticketId != null) 'ticket_id': ticketId,
  };
}

/// `thread.read` (feature `chat_threads`): one thread of a channel, its root
/// message and its replies oldest first, as the reading board member sees them.
final class ThreadSnapshot {
  const ThreadSnapshot({
    required this.asOfSeq,
    required this.conversationId,
    this.readOnly,
    this.replies = const [],
    required this.root,
  });

  final int asOfSeq;
  /// The channel (or ticket's discussion) the thread is in.
  final String conversationId;
  /// The channel is closed (an archived project's, a ticket's on Done or
  /// archived): the thread can be read, not answered.
  final bool? readOnly;
  /// Its replies, oldest first (each with `thread_parent` set to the root).
  final List<MessageView> replies;
  /// The message that starts the thread, with its counters (`thread_unread`
  /// as it was before this read marked the thread read).
  final MessageView root;

  factory ThreadSnapshot.fromJson(Map<String, dynamic> json) => ThreadSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    conversationId: (json['conversation_id'] as String),
    readOnly: (json['read_only'] == null ? null : (json['read_only'] as bool)),
    replies: (json['replies'] == null ? const [] : (json['replies'] as List<dynamic>).map((x0) => MessageView.fromJson(x0 as Map<String, dynamic>)).toList()),
    root: MessageView.fromJson(json['root'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'conversation_id': conversationId,
    if (readOnly != null) 'read_only': readOnly,
    'replies': replies.map((x0) => x0.toJson()).toList(),
    'root': root.toJson(),
  };
}

/// An employee that waits to pick up a thread (U3).
final class ThreadWaiter {
  const ThreadWaiter({
    required this.behind,
    required this.employeeId,
    required this.employeeName,
    this.reason,
  });

  /// How many threads of this employee go first, counting what it is doing now.
  final int behind;
  final String employeeId;
  final String employeeName;
  /// What holds it back besides its turn: `limit` (a usage limit), `question`
  /// (the thread waits for a board member's answer). A string, so a new one never breaks a client.
  final String? reason;

  factory ThreadWaiter.fromJson(Map<String, dynamic> json) => ThreadWaiter(
    behind: (json['behind'] as num).toInt(),
    employeeId: (json['employee_id'] as String),
    employeeName: (json['employee_name'] as String),
    reason: (json['reason'] == null ? null : (json['reason'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'behind': behind,
    'employee_id': employeeId,
    'employee_name': employeeName,
    if (reason != null) 'reason': reason,
  };
}

/// `thread.waiting`: who waits to pick up a thread and what is scheduled in it (U3, R7).
final class ThreadWaitingSnapshot {
  const ThreadWaitingSnapshot({
    required this.asOfSeq,
    required this.scheduled,
    required this.waiters,
  });

  final int asOfSeq;
  final List<ScheduledMessageView> scheduled;
  final List<ThreadWaiter> waiters;

  factory ThreadWaitingSnapshot.fromJson(Map<String, dynamic> json) => ThreadWaitingSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    scheduled: (json['scheduled'] as List<dynamic>).map((x0) => ScheduledMessageView.fromJson(x0 as Map<String, dynamic>)).toList(),
    waiters: (json['waiters'] as List<dynamic>).map((x0) => ThreadWaiter.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'scheduled': scheduled.map((x0) => x0.toJson()).toList(),
    'waiters': waiters.map((x0) => x0.toJson()).toList(),
  };
}

/// Where one ticket stands right now (feature `activity_overview`).
final class TicketActivity {
  const TicketActivity({
    this.employeeId,
    this.employeeName,
    required this.sinceMs,
    required this.state,
    required this.ticketId,
    this.waitsFor,
  });

  final String? employeeId;
  final String? employeeName;
  final int sinceMs;
  /// running, waiting_slot, paused or waiting_other. Text, not an enum.
  final String state;
  final String ticketId;
  /// With waiting_other: board_member, colleague, approval, question, limit,
  /// login, later or handoff. Also slot or machine with waiting_slot.
  final String? waitsFor;

  factory TicketActivity.fromJson(Map<String, dynamic> json) => TicketActivity(
    employeeId: (json['employee_id'] == null ? null : (json['employee_id'] as String)),
    employeeName: (json['employee_name'] == null ? null : (json['employee_name'] as String)),
    sinceMs: (json['since_ms'] as num).toInt(),
    state: (json['state'] as String),
    ticketId: (json['ticket_id'] as String),
    waitsFor: (json['waits_for'] == null ? null : (json['waits_for'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
    if (employeeName != null) 'employee_name': employeeName,
    'since_ms': sinceMs,
    'state': state,
    'ticket_id': ticketId,
    if (waitsFor != null) 'waits_for': waitsFor,
  };
}

/// `ticket.get`: the ticket, its main task and every task linked to it.
final class TicketDetail {
  const TicketDetail({
    required this.asOfSeq,
    this.rootTask,
    required this.tasks,
    required this.ticket,
  });

  final int asOfSeq;
  final TaskView? rootTask;
  /// Every task with this ticket's id (main tasks and delegations), oldest first.
  final List<TaskView> tasks;
  final TicketView ticket;

  factory TicketDetail.fromJson(Map<String, dynamic> json) => TicketDetail(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    rootTask: (json['root_task'] == null ? null : TaskView.fromJson(json['root_task'] as Map<String, dynamic>)),
    tasks: (json['tasks'] as List<dynamic>).map((x0) => TaskView.fromJson(x0 as Map<String, dynamic>)).toList(),
    ticket: TicketView.fromJson(json['ticket'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    if (rootTask != null) 'root_task': rootTask?.toJson(),
    'tasks': tasks.map((x0) => x0.toJson()).toList(),
    'ticket': ticket.toJson(),
  };
}

/// `ticket.followers` (feature `ticket_follow`): who follows a ticket.
final class TicketFollowersView {
  const TicketFollowersView({
    required this.followers,
  });

  final List<FollowerView> followers;

  factory TicketFollowersView.fromJson(Map<String, dynamic> json) => TicketFollowersView(
    followers: (json['followers'] as List<dynamic>).map((x0) => FollowerView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'followers': followers.map((x0) => x0.toJson()).toList(),
  };
}

/// What sort of ticket it is (protocol 5, feature `quick_task`, ticket #53).
enum TicketKind {
  /// Triage, Plan review, delegation and Testing as the board has them.
  normal('normal'),
  /// A small piece of work: it starts in Doing with its chosen employee in Auto mode,
  /// never gets triage, Plan review or Testing, cannot be delegated, works on its own
  /// branch `quick/ticket-<N>` and is merged into the project's local main on Done.
  quick('quick');

  const TicketKind(this.wire);

  final String wire;

  static TicketKind fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown TicketKind value: $value'),
      );

  String toJson() => wire;
}

/// Why a ticket cannot run yet (`TicketView::needs`).
enum TicketNeed {
  /// `login_user` has no working Claude login (none, expired, or disabled):
  /// "<name> must connect their Claude login". Another board member may take
  /// it over with `ticket.use_my_login`.
  login('login');

  const TicketNeed(this.wire);

  final String wire;

  static TicketNeed fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown TicketNeed value: $value'),
      );

  String toJson() => wire;
}

/// A ticket's priority (B9). Normal unless chosen.
enum TicketPriority {
  high('high'),
  low('low'),
  normal('normal'),
  urgent('urgent');

  const TicketPriority(this.wire);

  final String wire;

  static TicketPriority fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown TicketPriority value: $value'),
      );

  String toJson() => wire;
}

/// A card on the board, and the fields of the ticket view.
final class TicketView {
  const TicketView({
    this.archivedAtMs,
    required this.attachments,
    required this.column,
    required this.columnId,
    this.conversationId,
    required this.createdAtMs,
    required this.createdByUser,
    required this.createdByUsername,
    this.deadlineMs,
    this.description,
    this.designUrl,
    this.doneAtMs,
    required this.id,
    this.kind,
    required this.labels,
    required this.loginUser,
    required this.loginUsername,
    this.mergeCommit,
    this.needs,
    required this.number,
    this.ownerEmployeeId,
    this.ownerName,
    required this.position,
    required this.priority,
    required this.projectId,
    required this.projectName,
    this.responsibleDisabled,
    this.responsibleUserId,
    this.responsibleUsername,
    this.reviewFeedback,
    this.rootTaskId,
    this.rootTaskStatus,
    required this.run,
    this.startedAtMs,
    required this.title,
    required this.updatedAtMs,
    required this.version,
  });

  final int? archivedAtMs;
  /// Files attached to the ticket (fetched with `upload.get`).
  final List<MessageFileView> attachments;
  final ColumnKind column;
  final String columnId;
  /// The ticket's discussion (feature `ticket_discussion`): read it with
  /// `conversation.get`, write in it with `ticket.post`. Left out by older servers.
  final String? conversationId;
  final int createdAtMs;
  /// The board member who made it; never changes.
  final String createdByUser;
  final String createdByUsername;
  final int? deadlineMs;
  /// Markdown; none when empty.
  final String? description;
  /// The ticket's design, a Claude Design artifact (feature `design_links`). A link only:
  /// its runs read the artifact themselves. Left out when none and by older servers.
  final String? designUrl;
  final int? doneAtMs;
  final String id;
  /// `"quick"` for a quick task (feature `quick_task`, `TicketKind::as_str`); left out for a
  /// normal ticket, so its view is as before and an older client shows a quick task as a
  /// normal one. Text, not an enum: a view never holds an enum value an older client does not know (R1).
  final String? kind;
  final List<String> labels;
  /// Whose Claude login its runs use (protocol 5): the creator, unless a
  /// board member took it over on their own login (`ticket.use_my_login`).
  final String loginUser;
  final String loginUsername;
  /// The merge commit of a quick task accepted into the project's main (feature `quick_task`).
  final String? mergeCommit;
  /// What the ticket waits for before it can run; none: nothing.
  final TicketNeed? needs;
  /// Per company, from 1: what people call it (`#12`).
  final int number;
  /// The employee whose main task carries the ticket; none until chosen (by
  /// a board member or the dispatcher).
  final String? ownerEmployeeId;
  final String? ownerName;
  /// The order within its column, smallest first.
  final double position;
  final TicketPriority priority;
  final String projectId;
  final String projectName;
  /// The responsible board member is disabled: a client treats the ticket as
  /// having no active responsible, without a list of board members. Left out when false.
  final bool? responsibleDisabled;
  /// The board member who is responsible for the ticket (feature
  /// `ticket_responsible`), next to the employee who owns its main task. None:
  /// nobody. Left out then and by older servers.
  final String? responsibleUserId;
  /// The name of `responsible_user_id`, so a card needs no list of board members.
  final String? responsibleUsername;
  /// The last feedback of a board member (send back, revise plan).
  final String? reviewFeedback;
  /// The current main task; earlier ones stay linked through their `ticket_id`.
  final String? rootTaskId;
  final TaskStatus? rootTaskStatus;
  /// Mode and model of the main task (unset: the owner's own).
  final TaskRun run;
  final int? startedAtMs;
  final String title;
  final int updatedAtMs;
  final int version;

  factory TicketView.fromJson(Map<String, dynamic> json) => TicketView(
    archivedAtMs: (json['archived_at_ms'] == null ? null : (json['archived_at_ms'] as num).toInt()),
    attachments: (json['attachments'] as List<dynamic>).map((x0) => MessageFileView.fromJson(x0 as Map<String, dynamic>)).toList(),
    column: ColumnKind.fromJson(json['column'] as String),
    columnId: (json['column_id'] as String),
    conversationId: (json['conversation_id'] == null ? null : (json['conversation_id'] as String)),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    createdByUser: (json['created_by_user'] as String),
    createdByUsername: (json['created_by_username'] as String),
    deadlineMs: (json['deadline_ms'] == null ? null : (json['deadline_ms'] as num).toInt()),
    description: (json['description'] == null ? null : (json['description'] as String)),
    designUrl: (json['design_url'] == null ? null : (json['design_url'] as String)),
    doneAtMs: (json['done_at_ms'] == null ? null : (json['done_at_ms'] as num).toInt()),
    id: (json['id'] as String),
    kind: (json['kind'] == null ? null : (json['kind'] as String)),
    labels: (json['labels'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
    loginUser: (json['login_user'] as String),
    loginUsername: (json['login_username'] as String),
    mergeCommit: (json['merge_commit'] == null ? null : (json['merge_commit'] as String)),
    needs: (json['needs'] == null ? null : TicketNeed.fromJson(json['needs'] as String)),
    number: (json['number'] as num).toInt(),
    ownerEmployeeId: (json['owner_employee_id'] == null ? null : (json['owner_employee_id'] as String)),
    ownerName: (json['owner_name'] == null ? null : (json['owner_name'] as String)),
    position: (json['position'] as num).toDouble(),
    priority: TicketPriority.fromJson(json['priority'] as String),
    projectId: (json['project_id'] as String),
    projectName: (json['project_name'] as String),
    responsibleDisabled: (json['responsible_disabled'] == null ? null : (json['responsible_disabled'] as bool)),
    responsibleUserId: (json['responsible_user_id'] == null ? null : (json['responsible_user_id'] as String)),
    responsibleUsername: (json['responsible_username'] == null ? null : (json['responsible_username'] as String)),
    reviewFeedback: (json['review_feedback'] == null ? null : (json['review_feedback'] as String)),
    rootTaskId: (json['root_task_id'] == null ? null : (json['root_task_id'] as String)),
    rootTaskStatus: (json['root_task_status'] == null ? null : TaskStatus.fromJson(json['root_task_status'] as String)),
    run: TaskRun.fromJson(json['run'] as Map<String, dynamic>),
    startedAtMs: (json['started_at_ms'] == null ? null : (json['started_at_ms'] as num).toInt()),
    title: (json['title'] as String),
    updatedAtMs: (json['updated_at_ms'] as num).toInt(),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (archivedAtMs != null) 'archived_at_ms': archivedAtMs,
    'attachments': attachments.map((x0) => x0.toJson()).toList(),
    'column': column.toJson(),
    'column_id': columnId,
    if (conversationId != null) 'conversation_id': conversationId,
    'created_at_ms': createdAtMs,
    'created_by_user': createdByUser,
    'created_by_username': createdByUsername,
    if (deadlineMs != null) 'deadline_ms': deadlineMs,
    if (description != null) 'description': description,
    if (designUrl != null) 'design_url': designUrl,
    if (doneAtMs != null) 'done_at_ms': doneAtMs,
    'id': id,
    if (kind != null) 'kind': kind,
    'labels': labels,
    'login_user': loginUser,
    'login_username': loginUsername,
    if (mergeCommit != null) 'merge_commit': mergeCommit,
    if (needs != null) 'needs': needs?.toJson(),
    'number': number,
    if (ownerEmployeeId != null) 'owner_employee_id': ownerEmployeeId,
    if (ownerName != null) 'owner_name': ownerName,
    'position': position,
    'priority': priority.toJson(),
    'project_id': projectId,
    'project_name': projectName,
    if (responsibleDisabled != null) 'responsible_disabled': responsibleDisabled,
    if (responsibleUserId != null) 'responsible_user_id': responsibleUserId,
    if (responsibleUsername != null) 'responsible_username': responsibleUsername,
    if (reviewFeedback != null) 'review_feedback': reviewFeedback,
    if (rootTaskId != null) 'root_task_id': rootTaskId,
    if (rootTaskStatus != null) 'root_task_status': rootTaskStatus?.toJson(),
    'run': run.toJson(),
    if (startedAtMs != null) 'started_at_ms': startedAtMs,
    'title': title,
    'updated_at_ms': updatedAtMs,
    'version': version,
  };
}

/// `ticket.list`.
final class TicketsSnapshot {
  const TicketsSnapshot({
    required this.asOfSeq,
    required this.tickets,
  });

  final int asOfSeq;
  final List<TicketView> tickets;

  factory TicketsSnapshot.fromJson(Map<String, dynamic> json) => TicketsSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    tickets: (json['tickets'] as List<dynamic>).map((x0) => TicketView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'tickets': tickets.map((x0) => x0.toJson()).toList(),
  };
}

final class TimelineCard {
  const TimelineCard({
    required this.actor,
    required this.atMs,
    required this.correlationId,
    required this.detail,
    required this.kind,
    required this.label,
    required this.seq,
    this.taskId,
    required this.title,
  });

  final String actor;
  final int atMs;
  final String correlationId;
  final String detail;
  /// start | minor | decision | outcome | activity
  final String kind;
  final String label;
  final int seq;
  final String? taskId;
  final String title;

  factory TimelineCard.fromJson(Map<String, dynamic> json) => TimelineCard(
    actor: (json['actor'] as String),
    atMs: (json['at_ms'] as num).toInt(),
    correlationId: (json['correlation_id'] as String),
    detail: (json['detail'] as String),
    kind: (json['kind'] as String),
    label: (json['label'] as String),
    seq: (json['seq'] as num).toInt(),
    taskId: (json['task_id'] == null ? null : (json['task_id'] as String)),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'actor': actor,
    'at_ms': atMs,
    'correlation_id': correlationId,
    'detail': detail,
    'kind': kind,
    'label': label,
    'seq': seq,
    if (taskId != null) 'task_id': taskId,
    'title': title,
  };
}

final class TimelinePage {
  const TimelinePage({
    required this.cards,
    this.nextBeforeSeq,
  });

  final List<TimelineCard> cards;
  /// Pass as `before_seq` for the next page; `None` when exhausted.
  final int? nextBeforeSeq;

  factory TimelinePage.fromJson(Map<String, dynamic> json) => TimelinePage(
    cards: (json['cards'] as List<dynamic>).map((x0) => TimelineCard.fromJson(x0 as Map<String, dynamic>)).toList(),
    nextBeforeSeq: (json['next_before_seq'] == null ? null : (json['next_before_seq'] as num).toInt()),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'cards': cards.map((x0) => x0.toJson()).toList(),
    if (nextBeforeSeq != null) 'next_before_seq': nextBeforeSeq,
  };
}

final class TimelineQuery {
  const TimelineQuery({
    required this.activity,
    this.beforeSeq,
    this.correlationId,
    required this.limit,
    this.projectId,
  });

  /// Include routine runtime/tool events ("Activity").
  final bool activity;
  /// Keyset cursor: cards strictly older than this sequence number.
  final int? beforeSeq;
  /// Only the chain with this correlation id ("Waarom?" / "Wat volgde?").
  final String? correlationId;
  final int limit;
  final String? projectId;

  factory TimelineQuery.fromJson(Map<String, dynamic> json) => TimelineQuery(
    activity: (json['activity'] as bool),
    beforeSeq: (json['before_seq'] == null ? null : (json['before_seq'] as num).toInt()),
    correlationId: (json['correlation_id'] == null ? null : (json['correlation_id'] as String)),
    limit: (json['limit'] as num).toInt(),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'activity': activity,
    if (beforeSeq != null) 'before_seq': beforeSeq,
    if (correlationId != null) 'correlation_id': correlationId,
    'limit': limit,
    if (projectId != null) 'project_id': projectId,
  };
}

/// Tokens of one run or a sum of runs (feature `run_usage`). Every model call
/// sends the whole context again: `cached_tokens` is the part read from the prompt
/// cache (cheap), `input_tokens` the rest, of which `cache_creation_tokens` was
/// written to the cache (dearer than plain input).
final class TokenUsage {
  const TokenUsage({
    required this.cacheCreationTokens,
    required this.cachedTokens,
    this.costMicroUsd,
    required this.estimate,
    required this.inputTokens,
    this.modelCalls = 0,
    required this.outputTokens,
    required this.runs,
  });

  final int cacheCreationTokens;
  final int cachedTokens;
  /// Only when the provider reported a cost (micro-USD).
  final int? costMicroUsd;
  /// Some values are not a bill (e.g. subscription: API-equivalent value).
  final bool estimate;
  final int inputTokens;
  /// How many model calls those runs made (feature `run_cards`). 0: not known,
  /// for usage from before the calls were counted and by older servers.
  final int modelCalls;
  final int outputTokens;
  final int runs;

  factory TokenUsage.fromJson(Map<String, dynamic> json) => TokenUsage(
    cacheCreationTokens: (json['cache_creation_tokens'] as num).toInt(),
    cachedTokens: (json['cached_tokens'] as num).toInt(),
    costMicroUsd: (json['cost_micro_usd'] == null ? null : (json['cost_micro_usd'] as num).toInt()),
    estimate: (json['estimate'] as bool),
    inputTokens: (json['input_tokens'] as num).toInt(),
    modelCalls: (json['model_calls'] == null ? 0 : (json['model_calls'] as num).toInt()),
    outputTokens: (json['output_tokens'] as num).toInt(),
    runs: (json['runs'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'cache_creation_tokens': cacheCreationTokens,
    'cached_tokens': cachedTokens,
    if (costMicroUsd != null) 'cost_micro_usd': costMicroUsd,
    'estimate': estimate,
    'input_tokens': inputTokens,
    'model_calls': modelCalls,
    'output_tokens': outputTokens,
    'runs': runs,
  };
}

/// One line of `employee.tool_options`.
final class ToolOption {
  const ToolOption({
    required this.group,
    required this.label,
    required this.levels,
    required this.name,
  });

  final String group;
  final String label;
  /// The access levels that have this tool at all.
  final List<AccessLevel> levels;
  final String name;

  factory ToolOption.fromJson(Map<String, dynamic> json) => ToolOption(
    group: (json['group'] as String),
    label: (json['label'] as String),
    levels: (json['levels'] as List<dynamic>).map((x0) => AccessLevel.fromJson(x0 as String)).toList(),
    name: (json['name'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'group': group,
    'label': label,
    'levels': levels.map((x0) => x0.toJson()).toList(),
    'name': name,
  };
}

/// `employee.tool_options` (feature `employee_tools`).
final class ToolOptions {
  const ToolOptions({
    required this.tools,
  });

  final List<ToolOption> tools;

  factory ToolOptions.fromJson(Map<String, dynamic> json) => ToolOptions(
    tools: (json['tools'] as List<dynamic>).map((x0) => ToolOption.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'tools': tools.map((x0) => x0.toJson()).toList(),
  };
}

/// `EmployeeConfig::tool_policy` (C2).
final class ToolPolicy {
  const ToolPolicy({
    this.approvalTools = const [],
    this.mcpServers,
    this.tools,
  });

  /// The tools that always go to a board member first, in every mode:
  /// a tool of [`SWITCHABLE_TOOLS`], or a whole MCP server as `mcp__<server>__*`.
  final List<String> approvalTools;
  /// The MCP servers it may use, of those enabled server-wide. None: all of them.
  final List<String>? mcpServers;
  /// The switchable tools it has. None: all that its level gives.
  final List<String>? tools;

  factory ToolPolicy.fromJson(Map<String, dynamic> json) => ToolPolicy(
    approvalTools: (json['approval_tools'] == null ? const [] : (json['approval_tools'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    mcpServers: (json['mcp_servers'] == null ? null : (json['mcp_servers'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    tools: (json['tools'] == null ? null : (json['tools'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'approval_tools': approvalTools,
    if (mcpServers != null) 'mcp_servers': mcpServers,
    if (tools != null) 'tools': tools,
  };
}

final class TraceImage {
  const TraceImage({
    required this.bytes,
    required this.mediaType,
    required this.name,
  });

  final int bytes;
  final String mediaType;
  final String name;

  factory TraceImage.fromJson(Map<String, dynamic> json) => TraceImage(
    bytes: (json['bytes'] as num).toInt(),
    mediaType: (json['media_type'] as String),
    name: (json['name'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'bytes': bytes,
    'media_type': mediaType,
    'name': name,
  };
}

final class TracePart {
  const TracePart({
    required this.area,
    required this.chars,
    required this.label,
    required this.tokens,
  });

  /// system | prompt
  final String area;
  final int chars;
  /// E.g. "Instructions", "Memory", "Earlier conversation", "New message".
  final String label;
  /// Estimated (characters / 4).
  final int tokens;

  factory TracePart.fromJson(Map<String, dynamic> json) => TracePart(
    area: (json['area'] as String),
    chars: (json['chars'] as num).toInt(),
    label: (json['label'] as String),
    tokens: (json['tokens'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'area': area,
    'chars': chars,
    'label': label,
    'tokens': tokens,
  };
}

/// `trace.get_setting` / `trace.set_enabled`: whether runs keep a trace (feature `run_trace`).
final class TraceSetting {
  const TraceSetting({
    required this.enabled,
    required this.keepDays,
    required this.keepRuns,
  });

  final bool enabled;
  /// Traces older than this go.
  final int keepDays;
  /// Traces kept at most (the newest).
  final int keepRuns;

  factory TraceSetting.fromJson(Map<String, dynamic> json) => TraceSetting(
    enabled: (json['enabled'] as bool),
    keepDays: (json['keep_days'] as num).toInt(),
    keepRuns: (json['keep_runs'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'enabled': enabled,
    'keep_days': keepDays,
    'keep_runs': keepRuns,
  };
}

final class TraceStep {
  const TraceStep({
    required this.cacheCreationTokens,
    required this.cacheReadTokens,
    required this.inputTokens,
    required this.outputTokens,
    required this.subagent,
    required this.tools,
  });

  final int cacheCreationTokens;
  final int cacheReadTokens;
  /// Fresh input: neither read from nor written to the prompt cache.
  final int inputTokens;
  final int outputTokens;
  final bool subagent;
  /// The tools this call asked for.
  final List<String> tools;

  factory TraceStep.fromJson(Map<String, dynamic> json) => TraceStep(
    cacheCreationTokens: (json['cache_creation_tokens'] as num).toInt(),
    cacheReadTokens: (json['cache_read_tokens'] as num).toInt(),
    inputTokens: (json['input_tokens'] as num).toInt(),
    outputTokens: (json['output_tokens'] as num).toInt(),
    subagent: (json['subagent'] as bool),
    tools: (json['tools'] as List<dynamic>).map((x0) => (x0 as String)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'cache_creation_tokens': cacheCreationTokens,
    'cache_read_tokens': cacheReadTokens,
    'input_tokens': inputTokens,
    'output_tokens': outputTokens,
    'subagent': subagent,
    'tools': tools,
  };
}

/// Change the company's name, its dispatcher or its tester; left out: unchanged.
final class UpdateCompany {
  const UpdateCompany({
    this.dispatcher,
    this.name,
    this.tester,
  });

  final SetDispatcher? dispatcher;
  final String? name;
  /// Feature `board_testing`.
  final SetTester? tester;

  factory UpdateCompany.fromJson(Map<String, dynamic> json) => UpdateCompany(
    dispatcher: (json['dispatcher'] == null ? null : SetDispatcher.fromJson(json['dispatcher'] as Map<String, dynamic>)),
    name: (json['name'] == null ? null : (json['name'] as String)),
    tester: (json['tester'] == null ? null : SetTester.fromJson(json['tester'] as Map<String, dynamic>)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (dispatcher != null) 'dispatcher': dispatcher?.toJson(),
    if (name != null) 'name': name,
    if (tester != null) 'tester': tester?.toJson(),
  };
}

final class UpdateEmployee {
  const UpdateEmployee({
    this.accessLevel,
    this.description,
    required this.employeeId,
    this.instructions,
    required this.name,
    required this.role,
    this.sequentialTasks,
    this.title,
    this.toolPolicy,
  });

  /// What its runs may do with files and commands (C3). `None` leaves it as it is.
  final AccessLevel? accessLevel;
  /// What other employees read about this one (P7). `None` leaves it as it is;
  /// an empty text clears it.
  final String? description;
  final String employeeId;
  final String? instructions;
  final String name;
  final String role;
  /// Run its tasks one at a time; `None` leaves it as it is.
  final bool? sequentialTasks;
  /// `None` leaves the title as it is; an empty text clears it.
  final String? title;
  /// Which tools and MCP servers it has and which ask first (C2). `None` leaves it
  /// as it is; an empty policy is all of its level.
  final ToolPolicy? toolPolicy;

  factory UpdateEmployee.fromJson(Map<String, dynamic> json) => UpdateEmployee(
    accessLevel: (json['access_level'] == null ? null : AccessLevel.fromJson(json['access_level'] as String)),
    description: (json['description'] == null ? null : (json['description'] as String)),
    employeeId: (json['employee_id'] as String),
    instructions: (json['instructions'] == null ? null : (json['instructions'] as String)),
    name: (json['name'] as String),
    role: (json['role'] as String),
    sequentialTasks: (json['sequential_tasks'] == null ? null : (json['sequential_tasks'] as bool)),
    title: (json['title'] == null ? null : (json['title'] as String)),
    toolPolicy: (json['tool_policy'] == null ? null : ToolPolicy.fromJson(json['tool_policy'] as Map<String, dynamic>)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (accessLevel != null) 'access_level': accessLevel?.toJson(),
    if (description != null) 'description': description,
    'employee_id': employeeId,
    if (instructions != null) 'instructions': instructions,
    'name': name,
    'role': role,
    if (sequentialTasks != null) 'sequential_tasks': sequentialTasks,
    if (title != null) 'title': title,
    if (toolPolicy != null) 'tool_policy': toolPolicy?.toJson(),
  };
}

final class UpdateIdea {
  const UpdateIdea({
    this.addFiles = const [],
    this.confirmSecret,
    required this.ideaId,
    this.kind,
    this.note,
    this.removeFiles = const [],
    required this.title,
  });

  /// Files on the server to attach next to the ones the idea has.
  final List<String> addFiles;
  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final String ideaId;
  /// None keeps the idea's kind (older clients).
  final IdeaKind? kind;
  final String? note;
  /// Attachments to drop, by their `path` in `IdeaView::attachments`.
  final List<String> removeFiles;
  final String title;

  factory UpdateIdea.fromJson(Map<String, dynamic> json) => UpdateIdea(
    addFiles: (json['add_files'] == null ? const [] : (json['add_files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    ideaId: (json['idea_id'] as String),
    kind: (json['kind'] == null ? null : IdeaKind.fromJson(json['kind'] as String)),
    note: (json['note'] == null ? null : (json['note'] as String)),
    removeFiles: (json['remove_files'] == null ? const [] : (json['remove_files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'add_files': addFiles,
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'idea_id': ideaId,
    if (kind != null) 'kind': kind?.toJson(),
    if (note != null) 'note': note,
    'remove_files': removeFiles,
    'title': title,
  };
}

/// Change a project's settings; left out: unchanged. `dispatcher` with no
/// employee: the company's dispatcher again.
final class UpdateProject {
  const UpdateProject({
    this.dispatcher,
    required this.projectId,
    this.styleguideUrl,
  });

  final SetDispatcher? dispatcher;
  final String projectId;
  /// The workspace's styleguide: a Claude Design artifact link
  /// (`ticket::valid_artifact_url`, feature `design_links`); an empty text clears it.
  final String? styleguideUrl;

  factory UpdateProject.fromJson(Map<String, dynamic> json) => UpdateProject(
    dispatcher: (json['dispatcher'] == null ? null : SetDispatcher.fromJson(json['dispatcher'] as Map<String, dynamic>)),
    projectId: (json['project_id'] as String),
    styleguideUrl: (json['styleguide_url'] == null ? null : (json['styleguide_url'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (dispatcher != null) 'dispatcher': dispatcher?.toJson(),
    'project_id': projectId,
    if (styleguideUrl != null) 'styleguide_url': styleguideUrl,
  };
}

final class UpdateStatus {
  const UpdateStatus({
    this.app,
    required this.available,
    required this.checkedAtMs,
    required this.current,
    required this.enabled,
    this.error,
    this.latest,
    this.notes,
    this.reason,
    this.staged,
  });

  /// The fields above are the server's (service and aurelius-ctl). The
  /// desktop app is released separately: its own status, when this
  /// computer has the app installed. Absent from older servers.
  final UpdateStatus? app;
  final bool available;
  final int checkedAtMs;
  final String current;
  /// Only the installed production app updates itself.
  final bool enabled;
  final String? error;
  final String? latest;
  final String? notes;
  final String? reason;
  /// A downloaded and verified version, ready to apply.
  final String? staged;

  factory UpdateStatus.fromJson(Map<String, dynamic> json) => UpdateStatus(
    app: (json['app'] == null ? null : UpdateStatus.fromJson(json['app'] as Map<String, dynamic>)),
    available: (json['available'] as bool),
    checkedAtMs: (json['checked_at_ms'] as num).toInt(),
    current: (json['current'] as String),
    enabled: (json['enabled'] as bool),
    error: (json['error'] == null ? null : (json['error'] as String)),
    latest: (json['latest'] == null ? null : (json['latest'] as String)),
    notes: (json['notes'] == null ? null : (json['notes'] as String)),
    reason: (json['reason'] == null ? null : (json['reason'] as String)),
    staged: (json['staged'] == null ? null : (json['staged'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (app != null) 'app': app?.toJson(),
    'available': available,
    'checked_at_ms': checkedAtMs,
    'current': current,
    'enabled': enabled,
    if (error != null) 'error': error,
    if (latest != null) 'latest': latest,
    if (notes != null) 'notes': notes,
    if (reason != null) 'reason': reason,
    if (staged != null) 'staged': staged,
  };
}

/// Replace a schedule's template and recurrence (the project stays the same).
final class UpdateTaskSchedule {
  const UpdateTaskSchedule({
    this.acceptanceCriteria = const [],
    this.addFiles = const [],
    this.confirmSecret,
    required this.employeeId,
    required this.goal,
    required this.recurrence,
    this.removeFiles = const [],
    this.run,
    required this.scheduleId,
    required this.title,
  });

  final List<String> acceptanceCriteria;
  /// New files to attach (as `CreateTaskSchedule::files`).
  final List<String> addFiles;
  /// A board member confirmed that the text may hold a vault secret's value
  /// (after `looks_like_secret`, B10): it is stored with the value redacted.
  final bool? confirmSecret;
  final String employeeId;
  final String goal;
  final Recurrence recurrence;
  /// Attachments to drop, by their `path` in `TaskScheduleView::attachments`.
  final List<String> removeFiles;
  final TaskRun? run;
  final String scheduleId;
  final String title;

  factory UpdateTaskSchedule.fromJson(Map<String, dynamic> json) => UpdateTaskSchedule(
    acceptanceCriteria: (json['acceptance_criteria'] == null ? const [] : (json['acceptance_criteria'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    addFiles: (json['add_files'] == null ? const [] : (json['add_files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    confirmSecret: (json['confirm_secret'] == null ? null : (json['confirm_secret'] as bool)),
    employeeId: (json['employee_id'] as String),
    goal: (json['goal'] as String),
    recurrence: Recurrence.fromJson(json['recurrence'] as Map<String, dynamic>),
    removeFiles: (json['remove_files'] == null ? const [] : (json['remove_files'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    run: (json['run'] == null ? null : TaskRun.fromJson(json['run'] as Map<String, dynamic>)),
    scheduleId: (json['schedule_id'] as String),
    title: (json['title'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'acceptance_criteria': acceptanceCriteria,
    'add_files': addFiles,
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'employee_id': employeeId,
    'goal': goal,
    'recurrence': recurrence.toJson(),
    'remove_files': removeFiles,
    if (run != null) 'run': run?.toJson(),
    'schedule_id': scheduleId,
    'title': title,
  };
}

/// `upload.get`: a part of an uploaded file from `offset`, or its thumbnail.
final class UploadChunk {
  const UploadChunk({
    required this.byteLength,
    required this.dataBase64,
    required this.done,
    required this.id,
    required this.mediaType,
    required this.name,
    required this.offset,
    required this.thumbnail,
  });

  /// The whole file's size (the thumbnail's for a thumbnail).
  final int byteLength;
  final String dataBase64;
  /// No more parts after this one: `offset` + this part's size is `byte_length`.
  final bool done;
  final String id;
  /// Of the file (a thumbnail is always `image/jpeg`), guessed from its name.
  final String mediaType;
  final String name;
  final int offset;
  /// `data_base64` is a JPEG thumbnail, not the file.
  final bool thumbnail;

  factory UploadChunk.fromJson(Map<String, dynamic> json) => UploadChunk(
    byteLength: (json['byte_length'] as num).toInt(),
    dataBase64: (json['data_base64'] as String),
    done: (json['done'] as bool),
    id: (json['id'] as String),
    mediaType: (json['media_type'] as String),
    name: (json['name'] as String),
    offset: (json['offset'] as num).toInt(),
    thumbnail: (json['thumbnail'] as bool),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'byte_length': byteLength,
    'data_base64': dataBase64,
    'done': done,
    'id': id,
    'media_type': mediaType,
    'name': name,
    'offset': offset,
    'thumbnail': thumbnail,
  };
}

final class UsageDay {
  const UsageDay({
    required this.dayStartMs,
    required this.tokens,
  });

  final int dayStartMs;
  final int tokens;

  factory UsageDay.fromJson(Map<String, dynamic> json) => UsageDay(
    dayStartMs: (json['day_start_ms'] as num).toInt(),
    tokens: (json['tokens'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'day_start_ms': dayStartMs,
    'tokens': tokens,
  };
}

final class UsageHour {
  const UsageHour({
    this.cacheCreationTokens = 0,
    required this.cachedTokens,
    this.costMicroUsd,
    required this.estimate,
    required this.hourStartMs,
    required this.inputTokens,
    required this.outputTokens,
    required this.runs,
  });

  /// Part of `input_tokens` written to the prompt cache (feature `run_usage`).
  final int cacheCreationTokens;
  final int cachedTokens;
  final int? costMicroUsd;
  final bool estimate;
  final int hourStartMs;
  final int inputTokens;
  final int outputTokens;
  final int runs;

  factory UsageHour.fromJson(Map<String, dynamic> json) => UsageHour(
    cacheCreationTokens: (json['cache_creation_tokens'] == null ? 0 : (json['cache_creation_tokens'] as num).toInt()),
    cachedTokens: (json['cached_tokens'] as num).toInt(),
    costMicroUsd: (json['cost_micro_usd'] == null ? null : (json['cost_micro_usd'] as num).toInt()),
    estimate: (json['estimate'] as bool),
    hourStartMs: (json['hour_start_ms'] as num).toInt(),
    inputTokens: (json['input_tokens'] as num).toInt(),
    outputTokens: (json['output_tokens'] as num).toInt(),
    runs: (json['runs'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'cache_creation_tokens': cacheCreationTokens,
    'cached_tokens': cachedTokens,
    if (costMicroUsd != null) 'cost_micro_usd': costMicroUsd,
    'estimate': estimate,
    'hour_start_ms': hourStartMs,
    'input_tokens': inputTokens,
    'output_tokens': outputTokens,
    'runs': runs,
  };
}

/// One server-local day in hours: always 24 entries, oldest first.
final class UsageHours {
  const UsageHours({
    required this.dayStartMs,
    required this.hours,
  });

  final int dayStartMs;
  final List<UsageHour> hours;

  factory UsageHours.fromJson(Map<String, dynamic> json) => UsageHours(
    dayStartMs: (json['day_start_ms'] as num).toInt(),
    hours: (json['hours'] as List<dynamic>).map((x0) => UsageHour.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'day_start_ms': dayStartMs,
    'hours': hours.map((x0) => x0.toJson()).toList(),
  };
}

final class UsageRow {
  const UsageRow({
    this.cacheCreationTokens = 0,
    required this.cachedTokens,
    required this.connection,
    this.costMicroUsd,
    required this.estimate,
    required this.inputTokens,
    required this.model,
    required this.outputTokens,
    required this.provider,
    required this.runs,
  });

  /// Part of `input_tokens` written to the prompt cache (feature `run_usage`).
  final int cacheCreationTokens;
  final int cachedTokens;
  final String connection;
  final int? costMicroUsd;
  final bool estimate;
  final int inputTokens;
  final String model;
  final int outputTokens;
  final String provider;
  final int runs;

  factory UsageRow.fromJson(Map<String, dynamic> json) => UsageRow(
    cacheCreationTokens: (json['cache_creation_tokens'] == null ? 0 : (json['cache_creation_tokens'] as num).toInt()),
    cachedTokens: (json['cached_tokens'] as num).toInt(),
    connection: (json['connection'] as String),
    costMicroUsd: (json['cost_micro_usd'] == null ? null : (json['cost_micro_usd'] as num).toInt()),
    estimate: (json['estimate'] as bool),
    inputTokens: (json['input_tokens'] as num).toInt(),
    model: (json['model'] as String),
    outputTokens: (json['output_tokens'] as num).toInt(),
    provider: (json['provider'] as String),
    runs: (json['runs'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'cache_creation_tokens': cacheCreationTokens,
    'cached_tokens': cachedTokens,
    'connection': connection,
    if (costMicroUsd != null) 'cost_micro_usd': costMicroUsd,
    'estimate': estimate,
    'input_tokens': inputTokens,
    'model': model,
    'output_tokens': outputTokens,
    'provider': provider,
    'runs': runs,
  };
}

final class UsageSnapshot {
  const UsageSnapshot({
    required this.byDay,
    required this.bySource,
    this.byUser = const [],
    required this.windows,
  });

  /// Tokens per day, last 14 days (oldest first).
  final List<UsageDay> byDay;
  /// Per connection and model, last 30 days.
  final List<UsageRow> bySource;
  /// Per board member whose Claude login ran it, last 30 days (feature `run_cards`).
  /// Left out by older servers.
  final List<UsageUserRow> byUser;
  /// Today, 7 days, 30 days.
  final List<UsageWindow> windows;

  factory UsageSnapshot.fromJson(Map<String, dynamic> json) => UsageSnapshot(
    byDay: (json['by_day'] as List<dynamic>).map((x0) => UsageDay.fromJson(x0 as Map<String, dynamic>)).toList(),
    bySource: (json['by_source'] as List<dynamic>).map((x0) => UsageRow.fromJson(x0 as Map<String, dynamic>)).toList(),
    byUser: (json['by_user'] == null ? const [] : (json['by_user'] as List<dynamic>).map((x0) => UsageUserRow.fromJson(x0 as Map<String, dynamic>)).toList()),
    windows: (json['windows'] as List<dynamic>).map((x0) => UsageWindow.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'by_day': byDay.map((x0) => x0.toJson()).toList(),
    'by_source': bySource.map((x0) => x0.toJson()).toList(),
    'by_user': byUser.map((x0) => x0.toJson()).toList(),
    'windows': windows.map((x0) => x0.toJson()).toList(),
  };
}

/// `usage.get_totals`: what a conversation or a task used (feature `run_usage`).
final class UsageTotals {
  const UsageTotals({
    required this.own,
    required this.subtasks,
    required this.total,
  });

  /// The runs of the conversation itself (a task: of its conversation).
  final TokenUsage own;
  /// Tasks given away from it (delegated, handed over, started from it), each
  /// with its own sub-tasks included.
  final List<SubtaskUsage> subtasks;
  /// `own` plus every subtask.
  final TokenUsage total;

  factory UsageTotals.fromJson(Map<String, dynamic> json) => UsageTotals(
    own: TokenUsage.fromJson(json['own'] as Map<String, dynamic>),
    subtasks: (json['subtasks'] as List<dynamic>).map((x0) => SubtaskUsage.fromJson(x0 as Map<String, dynamic>)).toList(),
    total: TokenUsage.fromJson(json['total'] as Map<String, dynamic>),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'own': own.toJson(),
    'subtasks': subtasks.map((x0) => x0.toJson()).toList(),
    'total': total.toJson(),
  };
}

/// One board member's share of the usage (`UsageSnapshot::by_user`).
final class UsageUserRow {
  const UsageUserRow({
    required this.cacheCreationTokens,
    required this.cachedTokens,
    this.costMicroUsd,
    required this.estimate,
    required this.inputTokens,
    this.loginUser,
    this.modelCalls = 0,
    required this.outputTokens,
    required this.runs,
    this.username,
  });

  final int cacheCreationTokens;
  final int cachedTokens;
  final int? costMicroUsd;
  final bool estimate;
  final int inputTokens;
  /// `runs.login_user`; none: runs on no one's login (an API key, a background run).
  final String? loginUser;
  final int modelCalls;
  final int outputTokens;
  final int runs;
  /// That board member's name; none when there is none or they are gone.
  final String? username;

  factory UsageUserRow.fromJson(Map<String, dynamic> json) => UsageUserRow(
    cacheCreationTokens: (json['cache_creation_tokens'] as num).toInt(),
    cachedTokens: (json['cached_tokens'] as num).toInt(),
    costMicroUsd: (json['cost_micro_usd'] == null ? null : (json['cost_micro_usd'] as num).toInt()),
    estimate: (json['estimate'] as bool),
    inputTokens: (json['input_tokens'] as num).toInt(),
    loginUser: (json['login_user'] == null ? null : (json['login_user'] as String)),
    modelCalls: (json['model_calls'] == null ? 0 : (json['model_calls'] as num).toInt()),
    outputTokens: (json['output_tokens'] as num).toInt(),
    runs: (json['runs'] as num).toInt(),
    username: (json['username'] == null ? null : (json['username'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'cache_creation_tokens': cacheCreationTokens,
    'cached_tokens': cachedTokens,
    if (costMicroUsd != null) 'cost_micro_usd': costMicroUsd,
    'estimate': estimate,
    'input_tokens': inputTokens,
    if (loginUser != null) 'login_user': loginUser,
    'model_calls': modelCalls,
    'output_tokens': outputTokens,
    'runs': runs,
    if (username != null) 'username': username,
  };
}

final class UsageWindow {
  const UsageWindow({
    this.cacheCreationTokens = 0,
    required this.cachedTokens,
    this.costMicroUsd,
    required this.estimate,
    required this.inputTokens,
    required this.label,
    required this.outputTokens,
    required this.runs,
  });

  /// Part of `input_tokens` written to the prompt cache (feature `run_usage`).
  final int cacheCreationTokens;
  final int cachedTokens;
  /// Only when the provider reported a cost (micro-USD).
  final int? costMicroUsd;
  /// Some values are not a bill (e.g. subscription: API-equivalent value).
  final bool estimate;
  final int inputTokens;
  final String label;
  final int outputTokens;
  final int runs;

  factory UsageWindow.fromJson(Map<String, dynamic> json) => UsageWindow(
    cacheCreationTokens: (json['cache_creation_tokens'] == null ? 0 : (json['cache_creation_tokens'] as num).toInt()),
    cachedTokens: (json['cached_tokens'] as num).toInt(),
    costMicroUsd: (json['cost_micro_usd'] == null ? null : (json['cost_micro_usd'] as num).toInt()),
    estimate: (json['estimate'] as bool),
    inputTokens: (json['input_tokens'] as num).toInt(),
    label: (json['label'] as String),
    outputTokens: (json['output_tokens'] as num).toInt(),
    runs: (json['runs'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'cache_creation_tokens': cacheCreationTokens,
    'cached_tokens': cachedTokens,
    if (costMicroUsd != null) 'cost_micro_usd': costMicroUsd,
    'estimate': estimate,
    'input_tokens': inputTokens,
    'label': label,
    'output_tokens': outputTokens,
    'runs': runs,
  };
}

/// `user.create`. The password is never printed.
final class UserCreateParams {
  const UserCreateParams({
    required this.password,
    required this.username,
  });

  final String password;
  final String username;

  factory UserCreateParams.fromJson(Map<String, dynamic> json) => UserCreateParams(
    password: (json['password'] as String),
    username: (json['username'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'password': password,
    'username': username,
  };
}

/// A board member as an item or an audit row names them.
final class UserRef {
  const UserRef({
    required this.id,
    required this.username,
  });

  final String id;
  final String username;

  factory UserRef.fromJson(Map<String, dynamic> json) => UserRef(
    id: (json['id'] as String),
    username: (json['username'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'id': id,
    'username': username,
  };
}

/// A board member as every client sees it; never the password's hash.
final class UserView {
  const UserView({
    required this.createdAtMs,
    this.createdByUser,
    this.disabledAtMs,
    required this.id,
    required this.username,
  });

  final int createdAtMs;
  /// None: made in the onboarding, without an active board member.
  final String? createdByUser;
  /// Disabled (`user.disable`): cannot sign in; tickets keep their creator.
  final int? disabledAtMs;
  final String id;
  final String username;

  factory UserView.fromJson(Map<String, dynamic> json) => UserView(
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    createdByUser: (json['created_by_user'] == null ? null : (json['created_by_user'] as String)),
    disabledAtMs: (json['disabled_at_ms'] == null ? null : (json['disabled_at_ms'] as num).toInt()),
    id: (json['id'] as String),
    username: (json['username'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'created_at_ms': createdAtMs,
    if (createdByUser != null) 'created_by_user': createdByUser,
    if (disabledAtMs != null) 'disabled_at_ms': disabledAtMs,
    'id': id,
    'username': username,
  };
}

/// `user.list`, oldest first.
final class UsersSnapshot {
  const UsersSnapshot({
    required this.users,
  });

  final List<UserView> users;

  factory UsersSnapshot.fromJson(Map<String, dynamic> json) => UsersSnapshot(
    users: (json['users'] as List<dynamic>).map((x0) => UserView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'users': users.map((x0) => x0.toJson()).toList(),
  };
}

/// Who did it: a board member (on a device), or an employee in a run.
sealed class VaultActor {
  const VaultActor();

  factory VaultActor.fromJson(Map<String, dynamic> json) {
    final tag = json['type'] as String;
    switch (tag) {
      case 'board_member':
        return VaultActorBoardMember(
          user: UserRef.fromJson(json['user'] as Map<String, dynamic>),
        );
      case 'employee':
        return VaultActorEmployee(
          employeeId: (json['employee_id'] as String),
          name: (json['name'] as String),
          runId: (json['run_id'] == null ? null : (json['run_id'] as String)),
          taskId: (json['task_id'] == null ? null : (json['task_id'] as String)),
          ticketId: (json['ticket_id'] == null ? null : (json['ticket_id'] as String)),
        );
      default:
        throw FormatException('Unknown VaultActor type: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

final class VaultActorBoardMember extends VaultActor {
  const VaultActorBoardMember({
    required this.user,
  });

  final UserRef user;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'board_member',
      'user': user.toJson(),
    };
}

final class VaultActorEmployee extends VaultActor {
  const VaultActorEmployee({
    required this.employeeId,
    required this.name,
    this.runId,
    this.taskId,
    this.ticketId,
  });

  final String employeeId;
  final String name;
  final String? runId;
  final String? taskId;
  final String? ticketId;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'type': 'employee',
      'employee_id': employeeId,
      'name': name,
      if (runId != null) 'run_id': runId,
      if (taskId != null) 'task_id': taskId,
      if (ticketId != null) 'ticket_id': ticketId,
    };
}

/// What an audit row records. Never a value, also not an old or a new one.
enum VaultAuditAction {
  archive('archive'),
  /// A board member copied it (`vault.reveal` copy).
  copy('copy'),
  create('create'),
  /// A run got it as an environment variable (one row per run and item).
  deliver('deliver'),
  /// Name, env name, description or kind (`changed_fields`).
  edit('edit'),
  /// A board member exported the vault key (no item).
  keyExport('key_export'),
  /// A new value (for a secret: "Replaced").
  replace('replace'),
  /// A board member showed the value (`vault.reveal` show).
  reveal('reveal'),
  /// A run read a text or url with `vault_read`.
  toolRead('tool_read');

  const VaultAuditAction(this.wire);

  final String wire;

  static VaultAuditAction fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown VaultAuditAction value: $value'),
      );

  String toJson() => wire;
}

/// `vault.audit`: newest first.
final class VaultAuditPage {
  const VaultAuditPage({
    required this.more,
    required this.rows,
  });

  /// Older rows exist: ask again with `before` = the last row's id.
  final bool more;
  final List<VaultAuditRow> rows;

  factory VaultAuditPage.fromJson(Map<String, dynamic> json) => VaultAuditPage(
    more: (json['more'] as bool),
    rows: (json['rows'] as List<dynamic>).map((x0) => VaultAuditRow.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'more': more,
    'rows': rows.map((x0) => x0.toJson()).toList(),
  };
}

/// One row of the audit log (append-only).
final class VaultAuditRow {
  const VaultAuditRow({
    required this.action,
    required this.actor,
    required this.atMs,
    this.changedFields = const [],
    required this.id,
    this.itemId,
    this.itemName,
    this.projectId,
    this.via,
  });

  final VaultAuditAction action;
  final VaultActor actor;
  final int atMs;
  /// `edit`: which fields changed (`name`, `env_name`, `description`, `kind`); never their values.
  final List<String> changedFields;
  /// Newest is highest; `vault.audit { before }` pages on it.
  final int id;
  /// None: a key export.
  final String? itemId;
  /// The item's name at the time (also for an archived item).
  final String? itemName;
  /// The item's scope; none: the company (or a key export).
  final String? projectId;
  /// The board member's device (the session's name, "Desktop · dev-laptop").
  final String? via;

  factory VaultAuditRow.fromJson(Map<String, dynamic> json) => VaultAuditRow(
    action: VaultAuditAction.fromJson(json['action'] as String),
    actor: VaultActor.fromJson(json['actor'] as Map<String, dynamic>),
    atMs: (json['at_ms'] as num).toInt(),
    changedFields: (json['changed_fields'] == null ? const [] : (json['changed_fields'] as List<dynamic>).map((x0) => (x0 as String)).toList()),
    id: (json['id'] as num).toInt(),
    itemId: (json['item_id'] == null ? null : (json['item_id'] as String)),
    itemName: (json['item_name'] == null ? null : (json['item_name'] as String)),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
    via: (json['via'] == null ? null : (json['via'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'action': action.toJson(),
    'actor': actor.toJson(),
    'at_ms': atMs,
    'changed_fields': changedFields,
    'id': id,
    if (itemId != null) 'item_id': itemId,
    if (itemName != null) 'item_name': itemName,
    if (projectId != null) 'project_id': projectId,
    if (via != null) 'via': via,
  };
}

/// `vault.create`. The value is never printed.
final class VaultCreateParams {
  const VaultCreateParams({
    this.description,
    required this.envName,
    required this.kind,
    required this.name,
    this.projectId,
    required this.value,
  });

  final String? description;
  final String envName;
  final VaultKind kind;
  final String name;
  /// None: the company's.
  final String? projectId;
  final String value;

  factory VaultCreateParams.fromJson(Map<String, dynamic> json) => VaultCreateParams(
    description: (json['description'] == null ? null : (json['description'] as String)),
    envName: (json['env_name'] as String),
    kind: VaultKind.fromJson(json['kind'] as String),
    name: (json['name'] as String),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
    value: (json['value'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (description != null) 'description': description,
    'env_name': envName,
    'kind': kind.toJson(),
    'name': name,
    if (projectId != null) 'project_id': projectId,
    'value': value,
  };
}

/// One item as every client sees it.
final class VaultItemView {
  const VaultItemView({
    required this.createdAtMs,
    required this.createdBy,
    this.description,
    required this.envName,
    required this.id,
    required this.kind,
    required this.name,
    this.overridesCompany,
    this.projectId,
    this.undecryptable,
    required this.updatedAtMs,
    required this.updatedBy,
    this.value,
    required this.version,
  });

  final int createdAtMs;
  final UserRef createdBy;
  final String? description;
  /// How a run gets it (UPPER_SNAKE, unique in the scope): the environment
  /// variable, and the name in the vault tools.
  final String envName;
  final String id;
  final VaultKind kind;
  final String name;
  /// A project item whose env name a company item has too: in runs of the
  /// project this one wins (pill "overrides Company", B4).
  final bool? overridesCompany;
  /// None: the company's.
  final String? projectId;
  /// The server's vault key is missing or does not fit this value (a restore
  /// on another machine without the key, MU4-04 AC3): the client shows
  /// "Cannot be decrypted" instead of a value. The rest still works.
  final bool? undecryptable;
  final int updatedAtMs;
  final UserRef updatedBy;
  /// The value of a text or url item; never a secret's (`vault.reveal`), and
  /// none when it cannot be decrypted.
  final String? value;
  /// Goes up with every change (the envelope's `expected_version`).
  final int version;

  factory VaultItemView.fromJson(Map<String, dynamic> json) => VaultItemView(
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    createdBy: UserRef.fromJson(json['created_by'] as Map<String, dynamic>),
    description: (json['description'] == null ? null : (json['description'] as String)),
    envName: (json['env_name'] as String),
    id: (json['id'] as String),
    kind: VaultKind.fromJson(json['kind'] as String),
    name: (json['name'] as String),
    overridesCompany: (json['overrides_company'] == null ? null : (json['overrides_company'] as bool)),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
    undecryptable: (json['undecryptable'] == null ? null : (json['undecryptable'] as bool)),
    updatedAtMs: (json['updated_at_ms'] as num).toInt(),
    updatedBy: UserRef.fromJson(json['updated_by'] as Map<String, dynamic>),
    value: (json['value'] == null ? null : (json['value'] as String)),
    version: (json['version'] as num).toInt(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'created_at_ms': createdAtMs,
    'created_by': createdBy.toJson(),
    if (description != null) 'description': description,
    'env_name': envName,
    'id': id,
    'kind': kind.toJson(),
    'name': name,
    if (overridesCompany != null) 'overrides_company': overridesCompany,
    if (projectId != null) 'project_id': projectId,
    if (undecryptable != null) 'undecryptable': undecryptable,
    'updated_at_ms': updatedAtMs,
    'updated_by': updatedBy.toJson(),
    if (value != null) 'value': value,
    'version': version,
  };
}

/// `vault.key_export`: the server's vault key, to keep apart (a restore on
/// another machine needs it, B8). Never printed.
final class VaultKeyExport {
  const VaultKeyExport({
    required this.key,
    required this.keyId,
  });

  /// The key as one line: `aurelius-vault-key:v1:<env>:<key_id>:<base64url>`
  /// (`<env>` `dev` or `prod`, the 32 bytes in base64url without padding).
  /// What `aurelius-daemon vault-key import` reads.
  final String key;
  /// The key's fingerprint: what `aurelius-daemon vault-key import` checks
  /// against the stored values.
  final String keyId;

  factory VaultKeyExport.fromJson(Map<String, dynamic> json) => VaultKeyExport(
    key: (json['key'] as String),
    keyId: (json['key_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'key': key,
    'key_id': keyId,
  };
}

/// What an item holds. Text and url may change into each other; a secret stays
/// a secret, and nothing becomes one (`vault_kind_locked`, B7).
enum VaultKind {
  /// Masked everywhere; reaches runs only as an environment variable.
  secret('secret'),
  text('text'),
  /// `https://` or `http://`.
  url('url');

  const VaultKind(this.wire);

  final String wire;

  static VaultKind fromJson(String value) => values.firstWhere(
        (e) => e.wire == value,
        orElse: () => throw FormatException('Unknown VaultKind value: $value'),
      );

  String toJson() => wire;
}

/// `vault.list`: the items of the scope asked for (by name), and for a project
/// the company's apart (a run of the project gets those too, MU4-02 AC1).
final class VaultList {
  const VaultList({
    this.companyItems = const [],
    this.counts = const [],
    required this.items,
    this.projectId,
  });

  /// For a project: the company's items (empty for the company itself).
  final List<VaultItemView> companyItems;
  /// Every scope that has items, the company first (zero items included for it).
  final List<VaultScopeCount> counts;
  final List<VaultItemView> items;
  /// The scope asked for; none: the company.
  final String? projectId;

  factory VaultList.fromJson(Map<String, dynamic> json) => VaultList(
    companyItems: (json['company_items'] == null ? const [] : (json['company_items'] as List<dynamic>).map((x0) => VaultItemView.fromJson(x0 as Map<String, dynamic>)).toList()),
    counts: (json['counts'] == null ? const [] : (json['counts'] as List<dynamic>).map((x0) => VaultScopeCount.fromJson(x0 as Map<String, dynamic>)).toList()),
    items: (json['items'] as List<dynamic>).map((x0) => VaultItemView.fromJson(x0 as Map<String, dynamic>)).toList(),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'company_items': companyItems.map((x0) => x0.toJson()).toList(),
    'counts': counts.map((x0) => x0.toJson()).toList(),
    'items': items.map((x0) => x0.toJson()).toList(),
    if (projectId != null) 'project_id': projectId,
  };
}

/// How many items a scope has (the scope list of the vault screen).
final class VaultScopeCount {
  const VaultScopeCount({
    required this.count,
    this.projectId,
  });

  final int count;
  /// None: the company.
  final String? projectId;

  factory VaultScopeCount.fromJson(Map<String, dynamic> json) => VaultScopeCount(
    count: (json['count'] as num).toInt(),
    projectId: (json['project_id'] == null ? null : (json['project_id'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'count': count,
    if (projectId != null) 'project_id': projectId,
  };
}

/// `vault.update`. The value is never printed.
final class VaultUpdateParams {
  const VaultUpdateParams({
    this.description,
    this.envName,
    required this.itemId,
    this.kind,
    this.name,
    this.value,
  });

  /// `Some("")` clears it.
  final String? description;
  final String? envName;
  final String itemId;
  final VaultKind? kind;
  final String? name;
  final String? value;

  factory VaultUpdateParams.fromJson(Map<String, dynamic> json) => VaultUpdateParams(
    description: (json['description'] == null ? null : (json['description'] as String)),
    envName: (json['env_name'] == null ? null : (json['env_name'] as String)),
    itemId: (json['item_id'] as String),
    kind: (json['kind'] == null ? null : VaultKind.fromJson(json['kind'] as String)),
    name: (json['name'] == null ? null : (json['name'] as String)),
    value: (json['value'] == null ? null : (json['value'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (description != null) 'description': description,
    if (envName != null) 'env_name': envName,
    'item_id': itemId,
    if (kind != null) 'kind': kind?.toJson(),
    if (name != null) 'name': name,
    if (value != null) 'value': value,
  };
}

/// `vault.reveal`: one item's value. Never printed.
final class VaultValue {
  const VaultValue({
    required this.itemId,
    required this.value,
  });

  final String itemId;
  final String value;

  factory VaultValue.fromJson(Map<String, dynamic> json) => VaultValue(
    itemId: (json['item_id'] as String),
    value: (json['value'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'item_id': itemId,
    'value': value,
  };
}

/// Probe a connection now (status, models) and record the result.
final class VerifyConnection {
  const VerifyConnection({
    required this.connectionId,
  });

  final String connectionId;

  factory VerifyConnection.fromJson(Map<String, dynamic> json) => VerifyConnection(
    connectionId: (json['connection_id'] as String),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'connection_id': connectionId,
  };
}

/// `user.whoami`: what the onboarding gate needs (also in the hello).
final class WhoAmI {
  const WhoAmI({
    required this.boardMembersExist,
    this.mustChangePassword,
    this.user,
  });

  final bool boardMembersExist;
  /// The session's board member signed in with a temporary password and must
  /// change it (`user.set_password`) before anything else works.
  final bool? mustChangePassword;
  /// The board member of this connection's session.
  final UserView? user;

  factory WhoAmI.fromJson(Map<String, dynamic> json) => WhoAmI(
    boardMembersExist: (json['board_members_exist'] as bool),
    mustChangePassword: (json['must_change_password'] == null ? null : (json['must_change_password'] as bool)),
    user: (json['user'] == null ? null : UserView.fromJson(json['user'] as Map<String, dynamic>)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'board_members_exist': boardMembersExist,
    if (mustChangePassword != null) 'must_change_password': mustChangePassword,
    if (user != null) 'user': user?.toJson(),
  };
}

/// One work session in `work_session.list`.
final class WorkSessionView {
  const WorkSessionView({
    this.branch,
    required this.conversationId,
    required this.createdAtMs,
    this.dispatch,
    required this.employeeId,
    required this.employeeName,
    required this.lastActivityMs,
    required this.projectId,
    required this.projectName,
    required this.running,
    required this.title,
    this.unseen,
    this.worktree,
  });

  final String? branch;
  final String conversationId;
  final int createdAtMs;
  /// The dispatcher still chooses who takes it, or chose nobody (feature
  /// `session_dispatch`); none: `employee_id` works in it. While it chooses,
  /// `employee_id` is the dispatcher.
  final SessionDispatch? dispatch;
  final String employeeId;
  final String employeeName;
  final int lastActivityMs;
  final String projectId;
  final String projectName;
  /// Whether a run of it is queued or working now.
  final bool running;
  /// Named after its first message, until a title made from its content
  /// replaces it after the first answer.
  final String title;
  /// A run of it ended after the reader last opened it (with `conversation.get`), so
  /// there is a finished answer they have not seen. Never while it runs; older
  /// servers leave it out, and a client then shows no mark.
  final bool? unseen;
  /// Its own worktree and branch; none when it works in the project's folder.
  final String? worktree;

  factory WorkSessionView.fromJson(Map<String, dynamic> json) => WorkSessionView(
    branch: (json['branch'] == null ? null : (json['branch'] as String)),
    conversationId: (json['conversation_id'] as String),
    createdAtMs: (json['created_at_ms'] as num).toInt(),
    dispatch: (json['dispatch'] == null ? null : SessionDispatch.fromJson(json['dispatch'] as String)),
    employeeId: (json['employee_id'] as String),
    employeeName: (json['employee_name'] as String),
    lastActivityMs: (json['last_activity_ms'] as num).toInt(),
    projectId: (json['project_id'] as String),
    projectName: (json['project_name'] as String),
    running: (json['running'] as bool),
    title: (json['title'] as String),
    unseen: (json['unseen'] == null ? null : (json['unseen'] as bool)),
    worktree: (json['worktree'] == null ? null : (json['worktree'] as String)),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    if (branch != null) 'branch': branch,
    'conversation_id': conversationId,
    'created_at_ms': createdAtMs,
    if (dispatch != null) 'dispatch': dispatch?.toJson(),
    'employee_id': employeeId,
    'employee_name': employeeName,
    'last_activity_ms': lastActivityMs,
    'project_id': projectId,
    'project_name': projectName,
    'running': running,
    'title': title,
    if (unseen != null) 'unseen': unseen,
    if (worktree != null) 'worktree': worktree,
  };
}

/// `work_session.list`: the reader's sessions, the latest activity first.
final class WorkSessionsSnapshot {
  const WorkSessionsSnapshot({
    required this.asOfSeq,
    required this.sessions,
  });

  final int asOfSeq;
  final List<WorkSessionView> sessions;

  factory WorkSessionsSnapshot.fromJson(Map<String, dynamic> json) => WorkSessionsSnapshot(
    asOfSeq: (json['as_of_seq'] as num).toInt(),
    sessions: (json['sessions'] as List<dynamic>).map((x0) => WorkSessionView.fromJson(x0 as Map<String, dynamic>)).toList(),
  );

  Map<String, dynamic> toJson() => <String, dynamic>{
    'as_of_seq': asOfSeq,
    'sessions': sessions.map((x0) => x0.toJson()).toList(),
  };
}

sealed class WriteResult {
  const WriteResult();

  factory WriteResult.fromJson(Map<String, dynamic> json) {
    final tag = json['kind'] as String;
    switch (tag) {
      case 'command':
        return WriteResultCommand(value: CommandResult.fromJson(json['value'] as Map<String, dynamic>));
      case 'host':
        return WriteResultHost(value: HostResult.fromJson(json['value'] as Map<String, dynamic>));
      default:
        throw FormatException('Unknown WriteResult kind: $tag');
    }
  }

  Map<String, dynamic> toJson();
}

final class WriteResultCommand extends WriteResult {
  const WriteResultCommand({
    required this.value,
  });

  final CommandResult value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'command',
      'value': value.toJson(),
    };
}

final class WriteResultHost extends WriteResult {
  const WriteResultHost({
    required this.value,
  });

  final HostResult value;

  @override
  Map<String, dynamic> toJson() => <String, dynamic>{
      'kind': 'host',
      'value': value.toJson(),
    };
}
