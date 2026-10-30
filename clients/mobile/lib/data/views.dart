import 'dart:convert';

import 'package:intl/intl.dart';

import 'protocol/protocol.dart';

export 'protocol/protocol.dart';

// What the app derives from the generated protocol types: labels, flags and
// small computations. The shapes themselves come from lib/data/protocol.

/// The order the app cycles and lists modes in.
const agentModes = [AgentMode.ask, AgentMode.acceptEdits, AgentMode.auto, AgentMode.plan];

extension AgentModeView on AgentMode {
  String get label => switch (this) {
        AgentMode.ask => 'Ask',
        AgentMode.acceptEdits => 'Accept edits',
        AgentMode.auto => 'Auto',
        AgentMode.plan => 'Plan',
      };

  AgentMode get next => agentModes[(agentModes.indexOf(this) + 1) % agentModes.length];
}

extension MemoryKindView on MemoryKind {
  String get label => switch (this) {
        MemoryKind.identity => 'Identity',
        MemoryKind.projectKnowledge => 'Project knowledge',
        MemoryKind.decision => 'Decision',
        MemoryKind.episodic => 'Episode',
        MemoryKind.conversationSummary => 'Summary',
      };
}

extension EmployeeSummaryView on EmployeeSummary {
  String get monogram => name.isEmpty ? '?' : String.fromCharCode(name.runes.first).toUpperCase();

  /// "Title · Role", the title first when it has one.
  String get roleLine => [if ((title ?? '').isNotEmpty) title!, if (role.isNotEmpty) role].join(' · ');

  bool isMain(String conversationId) => conversationId == this.conversationId;

  /// The thread's title, or "New conversation".
  String threadTitle(String conversationId) {
    final title = conversations.where((c) => c.id == conversationId).map((c) => c.title).firstOrNull ?? '';
    return title.isEmpty ? 'New conversation' : title;
  }

  EmployeeSummary withSequentialTasks(bool value) => EmployeeSummary(
        agentMode: agentMode,
        agents: agents,
        browserAllowed: browserAllowed,
        conversationId: conversationId,
        conversations: conversations,
        folder: folder,
        heartbeat: heartbeat,
        id: id,
        instructions: instructions,
        name: name,
        departmentId: departmentId,
        projectsActive: projectsActive,
        reportsTo: reportsTo,
        role: role,
        screenAllowed: screenAllowed,
        sequentialTasks: value,
        status: status,
        title: title,
        version: version,
        working: working,
      );
}

extension AgentWorkView on AgentView {
  bool same(AgentView o) =>
      kind == o.kind && name == o.name && detail == o.detail && taskId == o.taskId && runId == o.runId && conversationId == o.conversationId && sinceMs == o.sinceMs && prompt == o.prompt && doing == o.doing && steps == o.steps;

  String get label => switch (kind) {
        'conversation' => 'Answering',
        'planning' => 'Planning',
        'review' => 'Reviewing',
        'follow_up' => 'Following up',
        'worker' => 'Worker',
        'subagent' => 'Subagent',
        'delegated' => 'Delegated to',
        _ => 'Running',
      };
}

extension TaskViewState on TaskView {
  /// The user can give it to (another) employee while nothing runs.
  bool get canAssign => status == TaskStatus.draft || status == TaskStatus.queued || status == TaskStatus.waiting;

  bool get isDone => status == TaskStatus.succeeded || status == TaskStatus.failed || status == TaskStatus.cancelled;

  /// Ended, but a follow-up run is going (or waits in the queue). Older servers never send it.
  bool get hasLiveRun => isDone && liveRun != null;

  /// Not ended, or ended with a follow-up run still going.
  bool get isOpen => !isDone || hasLiveRun;

  /// At work or about to be: counts as Active. A live follow-up run beats Done/Failed/Cancelled.
  bool get isActive => isOpen && !needsYou && !waitsToStart;

  /// Queued but not starting now: in line behind the employee's other task, or
  /// scheduled for later. Not at work. Older servers never send a reason here.
  bool get waitsToStart => status == TaskStatus.queued && waitingReason != null;

  bool get needsYou => pendingApprovals > 0 || variantsReady || (status == TaskStatus.waiting && (waitingReason == 'input' || waitingReason == 'escalated'));

  /// Its variants are done and wait for the user to choose one (feature `task_variants`).
  bool get variantsReady => variantPhase == VariantPhase.ready;

  /// Tried in several ways and not yet settled.
  bool get tryingVariants => switch (variantPhase) {
        VariantPhase.proposing || VariantPhase.probing || VariantPhase.judging || VariantPhase.applying => true,
        _ => false,
      };

  /// The same labels as the desktop task list.
  String get statusLabel => switch (status) {
        TaskStatus.waiting => switch (waitingReason) {
            'input' => 'Waiting for you',
            'dependency' => children.isEmpty ? 'Waiting for another task' : 'Waiting for colleagues',
            'to_manager' => 'Asking ${delegatedBy ?? 'the one who gave it'}',
            'paused' => 'Paused',
            'quota' => limitWait(waitingUntilMs),
            'offline' => 'Waiting for connection',
            'escalated' => 'Your decision needed',
            'variants' => variantsReady ? 'Variants ready to compare' : 'Trying variants',
            _ => 'Waiting',
          },
        TaskStatus.running when pendingApprovals > 0 => 'Waiting for you in the Inbox',
        TaskStatus.running when tryingVariants => variantPhaseLabel(variantPhase!),
        TaskStatus.draft => 'Draft',
        TaskStatus.queued => switch ((waitingReason, startsAtMs)) {
            ('scheduled', final int ms) => 'Scheduled · starts ${DateFormat.Hm().format(DateTime.fromMillisecondsSinceEpoch(ms))}',
            ('scheduled', _) => 'Scheduled',
            (null, _) => 'Queued',
            _ => 'In line',
          },
        TaskStatus.running => 'Running',
        TaskStatus.review => 'In review',
        TaskStatus.succeeded => 'Done',
        TaskStatus.failed => 'Failed',
        TaskStatus.cancelled => 'Cancelled',
      };

  /// Mode, connection, model and effort chosen for the task, as set values only.
  Map<String, String> get runChoices => run.choices;
}

extension TaskRunView on TaskRun {
  /// The set fields as strings (mode, connection_id, model, effort); empty: as its employee.
  Map<String, String> get choices => {
        'mode': ?mode?.wire,
        'connection_id': ?connectionId,
        'model': ?model,
        'effort': ?effort,
      };

  static TaskRun fromChoices(Map<String, String>? choices) => TaskRun(
        mode: choices?['mode'] == null ? null : AgentMode.fromJson(choices!['mode']!),
        connectionId: choices?['connection_id'],
        model: choices?['model'],
        effort: choices?['effort'],
      );
}

/// A [TaskRun] from the choices a task form collected (see [TaskRun.choices]).
TaskRun? taskRunOf(Map<String, String>? choices) => choices == null || choices.isEmpty ? null : TaskRunView.fromChoices(choices);

extension MessageViewState on MessageView {
  bool get isUser => role == MessageRole.user;
  bool get isStreaming => status == MessageStatus.partial;

  MessageView withText(String next, {List<ActivityView>? activity, String? author}) => MessageView(
        activity: activity ?? this.activity,
        approvalId: approvalId,
        author: author ?? this.author,
        createdAtMs: createdAtMs,
        files: files,
        id: id,
        ordinal: ordinal,
        producedBy: producedBy,
        queued: queued,
        replyTo: replyTo,
        role: role,
        runId: runId,
        sources: sources,
        status: status,
        text: next,
        update: update,
        uploads: uploads,
      );

  /// The text split around the tools it used, in order: `String` pieces of
  /// Markdown and [ActivityView]s. Offsets are UTF-8 bytes (Rust strings).
  List<Object> get parts {
    if (activity.isEmpty) return [text];
    final sorted = [...activity]..sort((a, b) => a.offset.compareTo(b.offset));
    final parts = <Object>[];
    var pos = 0;
    for (final a in sorted) {
      final at = utf16Index(text, a.offset).clamp(pos, text.length);
      if (text.substring(pos, at).trim().isNotEmpty) parts.add(text.substring(pos, at));
      pos = at;
      parts.add(a);
    }
    if (text.substring(pos).trim().isNotEmpty) parts.add(text.substring(pos));
    return parts;
  }
}

/// The UTF-16 index in [text] of a UTF-8 byte offset (rounded down to a
/// character boundary).
int utf16Index(String text, int byteOffset) {
  var bytes = 0;
  var index = 0;
  for (final rune in text.runes) {
    final size = rune < 0x80 ? 1 : (rune < 0x800 ? 2 : (rune < 0x10000 ? 3 : 4));
    if (bytes + size > byteOffset) break;
    bytes += size;
    index += rune >= 0x10000 ? 2 : 1;
  }
  return index;
}

extension ActivityViewState on ActivityView {
  /// `null` while running.
  bool get running => ok == null;

  /// A field of the tool input (any type), or null.
  Object? inputValue(String key) {
    final value = input;
    return value is Map ? value[key] : null;
  }

  /// A string field of the tool input, or ''.
  String field(String key) {
    final value = input;
    return value is Map && value[key] is String ? value[key] as String : '';
  }
}

extension FileRefView on FileRef {
  bool get isImage => mediaType.startsWith('image/') && mediaType != 'image/svg+xml';
  bool get isText => mediaType.startsWith('text/') || mediaType == 'application/json';
  String get sizeLabel => size >= 1048576 ? '${(size / 1048576).toStringAsFixed(1)} MB' : '${(size / 1024).ceil()} KB';
}

/// One of Claude's questions: 2–4 options, one or (multi) several.
class Question {
  const Question({required this.text, required this.header, required this.multi, required this.options});

  factory Question.fromJson(Map<String, dynamic> j) => Question(
        text: j['question'] as String? ?? '',
        header: j['header'] as String? ?? '',
        multi: j['multiSelect'] as bool? ?? false,
        options: (j['options'] as List? ?? const [])
            .whereType<Map<String, dynamic>>()
            .map((o) => (label: o['label'] as String? ?? '', description: o['description'] as String? ?? ''))
            .toList(),
      );

  final String text;
  final String header;
  final bool multi;
  final List<({String label, String description})> options;
}

extension PlanDecisionView on PlanDecision {
  bool get approves => this == PlanDecision.approve || this == PlanDecision.approveEdits;
}

/// An MCP tool name (`mcp__<server>__<tool>`, optionally `claude.`-prefixed)
/// as "server › tool"; anything else comes back unchanged.
String mcpToolName(String tool) {
  final bare = tool.startsWith('claude.') ? tool.substring(7) : tool;
  if (!bare.startsWith('mcp__')) return tool;
  final rest = bare.substring(5);
  final split = rest.indexOf('__');
  if (split <= 0 || split + 2 >= rest.length) return rest;
  return '${rest.substring(0, split)} › ${rest.substring(split + 2)}';
}

extension ApprovalViewState on ApprovalView {
  static const questionTool = 'claude.AskUserQuestion';
  static const planTool = 'claude.ExitPlanMode';

  bool get isPending => status == ApprovalStatus.pending;

  /// Claude asks the user something (AskUserQuestion) instead of an action.
  bool get isQuestion => tool == questionTool;

  /// Claude hands in a plan (ExitPlanMode): approve, ask for changes, or reject.
  bool get isPlan => tool == planTool;

  /// The plan text (markdown) of a plan approval.
  String get planMarkdown {
    final args = arguments;
    final plan = args is Map ? args['plan'] : null;
    return plan is String ? plan : '';
  }

  /// The plan's first heading (or first line), for the inbox card.
  String get planHeadline {
    for (final line in planMarkdown.split('\n')) {
      final text = line.replaceFirst(RegExp(r'^\s*#+\s*'), '').trim();
      if (text.isNotEmpty) return text;
    }
    return 'A plan';
  }

  /// Claude in Chrome: the user's own browser (their logins).
  bool get isBrowser => tool.startsWith('claude.mcp__claude-in-chrome__');

  /// A screenshot (or the list of open windows) on the server's computer.
  bool get isScreen => tool == 'claude.mcp__aurelius__screenshot' || tool == 'claude.mcp__aurelius__list_windows';

  /// The MCP tool as "server › tool", for the 'Always allow' explanation.
  String get mcpToolLabel => mcpToolName(tool);

  List<Question> get questions {
    final args = arguments;
    final raw = args is Map ? args['questions'] : null;
    if (raw is! List) return const [];
    return raw.whereType<Map<String, dynamic>>().map(Question.fromJson).where((q) => q.text.isNotEmpty).toList();
  }

  /// One line, as on the desktop: "Alex wants to edit a file", "Alex asks you".
  String get title {
    if (isQuestion) return '$requester asks you';
    if (isPlan) return '$requester has a plan';
    if (isBrowser) return '$requester wants to use your browser';
    if (tool == 'claude.mcp__aurelius__screenshot') return '$requester wants to see your screen';
    if (tool == 'claude.mcp__aurelius__list_windows') return '$requester wants to see which windows are open';
    final native = tool.startsWith('claude.') ? tool.substring(7) : null;
    return switch (native) {
      'Write' => '$requester wants to write a file',
      'Edit' || 'MultiEdit' || 'NotebookEdit' => '$requester wants to edit a file',
      'Bash' || 'PowerShell' => '$requester wants to run a command',
      'WebFetch' => '$requester wants to open a web page',
      'WebSearch' => '$requester wants to search the web',
      null => '$requester wants to run $tool',
      final other => '$requester wants to use $other',
    };
  }

  String get effectLabel => switch (effect) {
        'read' => 'Read',
        'local_write' => 'Change files on the PC',
        'external_write' => 'Send or publish outside the PC',
        'execute' => 'Run a program',
        _ => effect,
      };

  String get argumentsPretty => const JsonEncoder.withIndent('  ').convert(arguments);

  String get shortDigest => actionDigest.length > 12 ? actionDigest.substring(0, 12) : actionDigest;
}

extension UsageWindowView on UsageWindow {
  int get tokens => inputTokens + outputTokens;
}

extension UsageHourView on UsageHour {
  int get tokens => inputTokens + outputTokens;
}

extension UsageHoursView on UsageHours {
  int get runs => hours.fold(0, (n, h) => n + h.runs);
  int get inputTokens => hours.fold(0, (n, h) => n + h.inputTokens);
  int get outputTokens => hours.fold(0, (n, h) => n + h.outputTokens);
  int get cachedTokens => hours.fold(0, (n, h) => n + h.cachedTokens);

  /// Only when some hour reported a cost.
  int? get costMicroUsd => hours.any((h) => h.costMicroUsd != null) ? hours.fold<int>(0, (n, h) => n + (h.costMicroUsd ?? 0)) : null;
  bool get estimate => hours.any((h) => h.estimate);
}

extension IdeaViewState on IdeaView {
  /// Noted by an employee rather than the user.
  bool get byEmployee => employeeId != null || author != 'You';
}

extension ProjectConversationView on ProjectConversationSummary {
  String get displayTitle => title.isNotEmpty ? title : 'New conversation';
}

// ---- recurring tasks ----

const weekdayNames = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

String _clock(int minute) => '${(minute ~/ 60).toString().padLeft(2, '0')}:${(minute % 60).toString().padLeft(2, '0')}';

extension RecurrenceView on Recurrence {
  /// "Every 6 hours", "Daily at 09:30", "Mon, Fri at 09:00".
  String get label => switch (this) {
        RecurrenceEveryHours(:final hours) => hours == 1 ? 'Every hour' : 'Every $hours hours',
        RecurrenceDaily(:final atMinute) => 'Daily at ${_clock(atMinute)}',
        RecurrenceWeekly(:final weekdays, :final atMinute) => () {
            final days = ([...weekdays]..sort()).where((d) => d >= 0 && d < 7).toList();
            final text = days.length == 7 ? 'Every day' : days.map((d) => weekdayNames[d]).join(', ');
            return '$text at ${_clock(atMinute)}';
          }(),
      };
}

// ---- load and power ----

extension LoadLimitsView on LoadLimits {
  bool same(LoadLimits o) => maxRuns == o.maxRuns && cpuHigh == o.cpuHigh && cpuLow == o.cpuLow && memMinMb == o.memMinMb && enabled == o.enabled;

  String? get problem {
    if (cpuHigh > 100) return 'CPU high can be at most 100%.';
    if (cpuLow >= cpuHigh) return 'CPU low must be below CPU high.';
    return null;
  }
}

extension LoadSnapshotView on LoadSnapshot {
  bool get gateHeld => gate == LoadGate.held;

  String? get waitingText => switch (hold?.kind) {
        null => null,
        LoadHoldKind.cpu => 'Server busy: CPU ${cpuPct.round()}%, waiting',
        LoadHoldKind.memory => 'Server busy: ${gigabytes(memFreeMb)} free, waiting',
        LoadHoldKind.maxRuns => 'Server busy: ${running == 1 ? '1 run' : '$running runs'} active, waiting',
        LoadHoldKind.disabledAll => 'Runs paused on the server',
      };
}

String gigabytes(int mb) {
  final gb = mb / 1024;
  return '${gb < 10 ? gb.toStringAsFixed(1) : gb.round()} GB';
}

const powerNeedsOwner = 'Shutting down the PC needs the Owner right on this phone.';

/// "Waiting for limit reset until 14:05", with the day when that is not today;
/// the plain text when the reset time is unknown. Same as desktop.
String limitWait(int? untilMs, {DateTime? now}) {
  if (untilMs == null) return 'Waiting for limit reset';
  final at = DateTime.fromMillisecondsSinceEpoch(untilMs);
  final n = now ?? DateTime.now();
  final today = at.year == n.year && at.month == n.month && at.day == n.day;
  return 'Waiting for limit reset until ${DateFormat(today ? 'HH:mm' : 'EEE d MMM HH:mm').format(at)}';
}

String countdownText(Duration left) {
  final s = left.isNegative ? 0 : (left.inMilliseconds / 1000).ceil();
  final h = s ~/ 3600, m = (s % 3600) ~/ 60, sec = s % 60;
  String two(int n) => n.toString().padLeft(2, '0');
  return h > 0 ? '$h:${two(m)}:${two(sec)}' : '$m:${two(sec)}';
}

/// "PC shuts down in 0:42" or, while waiting for idle, "PC shuts down when all work is done".
String pendingText(PendingShutdown p, int now) {
  final deadline = p.deadlineMs;
  if (deadline == null) return 'PC shuts down when all work is done';
  return 'PC shuts down in ${countdownText(Duration(milliseconds: deadline - now))}';
}

// ---- git ----

extension GitStatusState on GitStatusView {
  bool get clean => files == 0;

  /// "main", or "detached abc123".
  String? get label => branch ?? (head == null ? null : 'detached $head');

  /// What a tap copies: the branch name, else the commit.
  String get copyText => branch ?? head ?? '';

  bool same(GitStatusView? o) => o != null && o.branch == branch && o.head == head && o.files == files && o.insertions == insertions && o.deletions == deletions;
}

extension GitDiffState on GitDiffView {
  List<FileDiff> get fileDiffs => FileDiff.split(patch);
}

/// One file's part of a unified diff.
class FileDiff {
  const FileDiff({required this.path, required this.lines, required this.insertions, required this.deletions});

  final String path;

  /// Hunk headers and changed lines, or a note such as "Binary files differ".
  final List<String> lines;
  final int insertions;
  final int deletions;

  /// Split `git diff` output into files (`diff --git a/x b/x` starts one).
  static List<FileDiff> split(String patch) {
    final files = <FileDiff>[];
    String? path;
    var lines = <String>[];
    var inHunks = false;
    void flush() {
      if (path == null) return;
      while (lines.isNotEmpty && lines.last.isEmpty) {
        lines.removeLast();
      }
      final ins = lines.where((l) => l.startsWith('+')).length;
      final del = lines.where((l) => l.startsWith('-')).length;
      files.add(FileDiff(path: path, lines: lines, insertions: ins, deletions: del));
    }

    for (final raw in patch.split('\n')) {
      final line = raw.endsWith('\r') ? raw.substring(0, raw.length - 1) : raw;
      if (line.startsWith('diff --git ')) {
        flush();
        path = _path(line);
        lines = <String>[];
        inHunks = false;
      } else if (path == null) {
        continue;
      } else if (line.startsWith('@@')) {
        inHunks = true;
        lines.add(line);
      } else if (inHunks) {
        lines.add(line);
      } else if (line.startsWith('Binary files') || line.startsWith('rename ') || line.startsWith('deleted file') || line.startsWith('new file')) {
        lines.add(line);
      } else if (line.startsWith('+++ b/')) {
        path = line.substring(6);
      }
    }
    flush();
    return files;
  }

  static String _path(String header) {
    final rest = header.substring('diff --git '.length);
    final b = rest.lastIndexOf(' b/');
    return b >= 0 ? rest.substring(b + 3) : rest;
  }
}

// ---- markdown ----

extension MarkdownFileEntryView on MarkdownFileEntry {
  String get name => path.split('/').last;
  String get folder => path.contains('/') ? path.substring(0, path.lastIndexOf('/')) : '';
}

// ---- app keys ----

extension ApiKeyViewState on ApiKeyView {
  bool get revoked => revokedAtMs != null;
}

// ---- plugins ----

const pluginsNeedsApprove = 'Changing plugins needs the Approve right on this phone.';
const pluginHooksWarning = 'Runs hooks (commands on this PC)';
const pluginMcpNote = 'MCP servers not loaded';

extension PluginComponentsView on PluginComponents {
  /// "3 skills", "1 command", ...: one label per kind that is present.
  List<String> get chips => [
        if (skills.isNotEmpty) '${skills.length} ${skills.length == 1 ? 'skill' : 'skills'}',
        if (commands.isNotEmpty) '${commands.length} ${commands.length == 1 ? 'command' : 'commands'}',
        if (agents.isNotEmpty) '${agents.length} ${agents.length == 1 ? 'agent' : 'agents'}',
        if (hooks) 'hooks',
        if (mcpServers.isNotEmpty) 'MCP',
      ];
}

/// A readable line for a failed plugin call.
String pluginError(Object e, String fallback) {
  final text = e.toString().trim();
  return text.isEmpty ? fallback : text;
}

// ---- MCP ----

const mcpNeedsApprove = 'Turning MCP servers on or off needs the Approve right on this phone.';

extension McpServersView on McpServers {
  List<String> get enabledNames => [for (final s in servers) if (s.enabled) s.name, ...missing];
}

const mcpLoginNeedsApprove = 'Logging in to MCP servers needs the Approve right on this phone.';

extension McpServerAuthView on McpServerInfo {
  /// The login badge; `null` for servers without a login (stdio, a fixed header)
  /// and on older servers, which leave `auth` out.
  String? get authLabel => switch (auth) {
        McpAuthStatus.authorized => 'Connected',
        McpAuthStatus.needsLogin => 'Needs login',
        McpAuthStatus.unknown => 'Unknown',
        McpAuthStatus.none || null => null,
      };

  bool get canLogIn => auth == McpAuthStatus.needsLogin || auth == McpAuthStatus.unknown;
  bool get canLogOut => auth == McpAuthStatus.authorized;
}

/// Whether [text] is the address a login's browser ended on: the server's
/// `http://localhost:<port>/callback` with a `code` (or an `error`) in the query.
bool looksLikeMcpCallback(String text, {String? redirectUri}) {
  final uri = Uri.tryParse(text.trim());
  if (uri == null || !uri.hasQuery) return false;
  final q = uri.queryParameters;
  if (!q.containsKey('code') && !q.containsKey('error')) return false;
  if (redirectUri != null && text.trim().startsWith(redirectUri)) return true;
  return (uri.host == 'localhost' || uri.host == '127.0.0.1') && uri.path.endsWith('/callback');
}

/// What to say when a login ended without success.
String? mcpLoginProblem(McpLogin login) => switch (login.state) {
      McpLoginState.failed => login.error ?? 'The login failed.',
      McpLoginState.expired => 'This login expired. Start it again.',
      McpLoginState.pending || McpLoginState.succeeded => null,
    };

// ---- org chart ----

extension ProposalState on ProposalView {
  bool get isPending => status == ProposalStatus.pending;

  /// New instructions for an employee rather than a new colleague (feature `instruction_proposals`).
  bool get isInstructions => kind == ProposalKind.instructions;

  /// The employee's instructions changed after the proposal was written:
  /// approving would overwrite that change, so the server refuses it.
  bool get isStale => isInstructions && baseVersion != null && currentVersion != null && currentVersion != baseVersion;
}

// ---- Harness² ----

/// Server features of Harness² (`HelloResult.features`); without one its parts stay hidden.
abstract final class Harness2 {
  static const lessons = 'task_lessons';
  static const instructionProposals = 'instruction_proposals';
  static const variants = 'task_variants';
}

/// A task's variant phase in words.
String variantPhaseLabel(VariantPhase phase) => switch (phase) {
      VariantPhase.proposing => 'Thinking up variants',
      VariantPhase.probing => 'Trying variants',
      VariantPhase.judging => 'Comparing variants',
      VariantPhase.ready => 'Variants ready to compare',
      VariantPhase.applying => 'Applying the chosen variant',
      VariantPhase.done => 'Variant chosen',
      VariantPhase.stopped => 'Variants stopped',
      VariantPhase.failed => 'Variants failed',
    };

/// The cost note next to the variants picker: k variants use about k times the tokens.
String variantsCostNote(int count) => count <= 1 ? 'One run, as usual.' : 'Tries it $count ways side by side, then a judge compares: about $count× the tokens.';

extension TaskVariantViewText on TaskVariantView {
  /// "3 files · +40 −12".
  String get changeLine => '$files file${files == 1 ? '' : 's'} · +$insertions −$deletions';

  /// Tokens this trial used (input, cache writes and output).
  int get tokens => usage.inputTokens + usage.cacheCreationTokens + usage.outputTokens;

  bool get choosable => status == VariantStatus.done;
}

extension LessonLocatorText on LessonLocator {
  /// "confirmed 2×" once later tasks reinforced it; null before.
  String? get confirmedLine => reinforced > 0 ? 'confirmed $reinforced×' : null;
}

extension ReceiptView on Receipt {
  /// The result of a domain command, or `null` for a host write.
  CommandResult? get command => switch (result) {
        WriteResultCommand(:final value) => value,
        _ => null,
      };
}
