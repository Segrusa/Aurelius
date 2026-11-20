// Work sessions (server feature `work_sessions`): the rules of the Sessions tab,
// New session and the session screen, pure. Ported from the desktop's
// session-logic.ts and new-session-logic.ts, with the same words.

import '../../data/protocol/protocol.dart';

/// The server feature of work sessions.
const workSessionsFeature = 'work_sessions';

/// The server's dispatcher chooses who takes a session (`work_session.dispatch`).
const sessionDispatchFeature = 'session_dispatch';

/// The server takes files with a session's first message.
const chatFilesFeature = 'chat_files';

/// The Who of a session nobody was chosen for: the dispatcher chooses.
const dispatcherWho = 'dispatcher';

/// What a session's row signals: it works now, it waits for your choice of who
/// takes it, or it finished an answer you have not opened yet ([done], until you open it).
enum SessionSignal {
  working,
  needsYou,
  done;

  /// Working or needing you: what puts a row in the Active section. Done does not.
  bool get isActive => this != done;
}

/// A session works while a run of it is queued or under way, or while the
/// dispatcher chooses who takes it; it needs you once the dispatcher chose nobody.
/// Done only says something when neither of the others does.
SessionSignal? sessionSignal(WorkSessionView s) {
  if (s.running || s.dispatch == SessionDispatch.choosing) return SessionSignal.working;
  if (s.dispatch == SessionDispatch.nobody) return SessionSignal.needsYou;
  if (s.unseen ?? false) return SessionSignal.done;
  return null;
}

/// How many of these sessions work now (the Sessions header).
int activeSessions(Iterable<WorkSessionView> sessions) => sessions.where((s) => sessionSignal(s) == SessionSignal.working).length;

/// The newest activity first; the server sends them so, this keeps it so after a local change.
List<WorkSessionView> sessionsByActivity(Iterable<WorkSessionView> sessions) => [...sessions]..sort((a, b) => b.lastActivityMs.compareTo(a.lastActivityMs));

/// The sections of the Sessions tab, in their order: what works or waits for
/// your choice now, then by the day of the last activity (the week starts on Monday).
enum SessionPeriod {
  active('Active'),
  today('Today'),
  yesterday('Yesterday'),
  thisWeek('This week'),
  thisMonth('This month'),
  older('Older');

  const SessionPeriod(this.label);

  /// The section's header.
  final String label;
}

/// The section of something used last at [usedMs], at [now] (local time):
/// Active while it has a signal, else by the day it was last used.
SessionPeriod periodOf({required bool active, required int usedMs, required DateTime now}) {
  if (active) return SessionPeriod.active;
  final at = DateTime.fromMillisecondsSinceEpoch(usedMs);
  final today = DateTime(now.year, now.month, now.day);
  if (!at.isBefore(today)) return SessionPeriod.today;
  if (!at.isBefore(DateTime(now.year, now.month, now.day - 1))) return SessionPeriod.yesterday;
  if (!at.isBefore(DateTime(now.year, now.month, now.day - (now.weekday - DateTime.monday)))) return SessionPeriod.thisWeek;
  if (!at.isBefore(DateTime(now.year, now.month))) return SessionPeriod.thisMonth;
  return SessionPeriod.older;
}

/// The section of [s] at [now] (local time): Active while it has a signal, else by its last activity.
SessionPeriod sessionPeriod(WorkSessionView s, DateTime now, {int? usedMs}) =>
    periodOf(active: sessionSignal(s)?.isActive ?? false, usedMs: usedMs ?? s.lastActivityMs, now: now);

/// [items] in their sections, in the sections' order, each most recently used
/// first (desktop's byLastUse: ties keep their order). [usedAt] is when an
/// item was last used, [active] whether it signals now.
List<(SessionPeriod, List<T>)> sectionsByUse<T>(Iterable<T> items, DateTime now, {required int Function(T) usedAt, required bool Function(T) active}) {
  final ordered = [for (final (i, item) in items.indexed) (i, item, usedAt(item))]..sort((a, b) {
      final by = b.$3.compareTo(a.$3);
      return by != 0 ? by : a.$1.compareTo(b.$1);
    });
  final by = <SessionPeriod, List<T>>{};
  for (final (_, item, used) in ordered) {
    by.putIfAbsent(periodOf(active: active(item), usedMs: used, now: now), () => []).add(item);
  }
  return [for (final p in SessionPeriod.values) if (by[p] case final list?) (p, list)];
}

/// The non-empty sections in their order, each with its sessions latest
/// activity first: opening one does not move it up.
List<(SessionPeriod, List<WorkSessionView>)> sessionSections(Iterable<WorkSessionView> sessions, DateTime now) =>
    sectionsByUse(
      sessions,
      now,
      usedAt: (s) => s.lastActivityMs,
      active: (s) => sessionSignal(s)?.isActive ?? false,
    );

/// Who works in a session: its employee, the dispatcher at it while it chooses, or nobody yet.
String sessionWho(WorkSessionView s) => switch (s.dispatch) {
      SessionDispatch.choosing => '${s.employeeName} is choosing who takes it',
      SessionDispatch.nobody => 'Nobody yet',
      null => s.employeeName,
    };

/// What stands where the composer is while nobody works in the session yet; null once somebody does.
String? chooserText(WorkSessionView s) => switch (s.dispatch) {
      SessionDispatch.choosing => '${s.employeeName} is choosing who takes this session. You can also choose yourself.',
      SessionDispatch.nobody => 'The dispatcher chose nobody. Choose who takes this session.',
      null => null,
    };

/// The status line: a run queued or under way works; otherwise it is your turn.
String sessionStatus(bool running, SessionDispatch? dispatch) => switch (dispatch) {
      SessionDispatch.choosing => 'Dispatcher is choosing',
      SessionDispatch.nobody => 'Waiting for your choice',
      null => running ? 'Working' : 'Waiting for you',
    };

/// The session's worktree when it has one, else the project's own folder.
String sessionFolder(WorkSessionView s) {
  final worktree = s.worktree;
  return worktree == null || worktree.isEmpty ? 'Project folder' : worktree;
}

/// What a session start says under the composer.
String sessionNote(String? whoName, bool worktree) {
  final who = whoName ?? 'The employee';
  return worktree
      ? '$who works in a worktree of its own, on its own branch. No ticket: you talk with them in the session.'
      : "$who works in the project's folder itself. No ticket: you talk with them in the session.";
}

/// The dispatcher chooses who takes a session: its words after Start.
String dispatchedNote(bool worktree) => worktree
    ? 'The dispatcher chooses who takes it, in a worktree of its own on its own branch. No ticket: you talk with them in the session.'
    : "The dispatcher chooses who takes it, in the project's folder itself. No ticket: you talk with them in the session.";

/// The composer's hint on New session (desktop: placeholderOf("session")).
const newSessionHint = 'What do you want to work on?';

/// What is remembered per server: `project`, `recent`, `kind`, `who`, and the
/// run choices `model`, `effort` and `mode` of the last session start.
String sessionKey(String what, String hostId) => 'new_session.$what:$hostId';

/// How many projects the Chat tab offers as chips before "All projects…".
const projectRowsMax = 6;

/// Projects remembered by last use; more than the row shows, so a project picked once keeps its place.
const recentProjectsMax = 20;

/// Tickets the Chat tab offers to continue.
const continueMax = 3;

/// The columns of a ticket that is still being worked on.
const continueColumns = {ColumnKind.doing, ColumnKind.testing, ColumnKind.review};

/// The projects the Chat tab shows as chips (desktop projectRows): the most
/// recently used first, then in list order; the chosen one is always among them.
/// [more] when there are more than fit.
({List<T> rows, bool more}) projectRows<T>(List<T> shown, List<String> recent, String? chosen, {required String Function(T) id, int max = projectRowsMax}) {
  int rank(T p) {
    final at = recent.indexOf(id(p));
    return at < 0 ? recent.length : at;
  }

  final ordered = [for (final (i, p) in shown.indexed) (i, p)]..sort((a, b) {
      final by = rank(a.$2).compareTo(rank(b.$2));
      return by != 0 ? by : a.$1.compareTo(b.$1);
    });
  final rows = [for (final (_, p) in ordered.take(max)) p];
  final picked = shown.where((p) => id(p) == chosen).firstOrNull;
  if (picked != null && !rows.contains(picked)) rows[rows.length - 1] = picked;
  return (rows: rows, more: shown.length > max);
}

/// [recent] with [projectId] used now, newest first.
List<String> noteRecent(List<String> recent, String projectId) => [projectId, ...recent.where((r) => r != projectId)].take(recentProjectsMax).toList();

/// Your tickets to continue (desktop continueTickets): in Doing, Testing or Review, last updated first.
List<TicketView> continueTickets(Iterable<TicketView> tickets) =>
    ([for (final t in tickets) if (t.archivedAtMs == null && continueColumns.contains(t.column)) t]..sort((a, b) => b.updatedAtMs.compareTo(a.updatedAtMs))).take(continueMax).toList();

/// The pill on a ticket to continue.
String continueColumnLabel(ColumnKind column) => switch (column) {
      ColumnKind.doing => 'Doing',
      ColumnKind.testing => 'Testing',
      ColumnKind.review => 'Review',
      _ => column.wire,
    };

/// A remembered model, effort or mode while it is still [offered], else
/// `default`. Before the offered list has loaded ([loaded] false) the
/// remembered one is kept, so a slow start does not lose it.
String keptChoice(String? stored, List<String> offered, {bool loaded = true}) {
  if (stored == null || stored.isEmpty) return 'default';
  return !loaded || offered.contains(stored) ? stored : 'default';
}

/// The last used project while it is still open, else the only open one, else none (the board member picks).
String? defaultSessionProject(List<String> open, String? last) {
  if (last != null && open.contains(last)) return last;
  return open.length == 1 ? open.single : null;
}

/// The employee chosen last while still active; else the Dispatcher where the
/// server has one for sessions; else nobody (the board member chooses).
String? defaultWho({required String? last, required Set<String> active, required bool dispatchOn}) {
  if (last != null && active.contains(last)) return last;
  return dispatchOn ? dispatcherWho : null;
}

/// Why Start cannot go yet, or null. The server checks again and says why in its own words.
String? startProblem({
  required bool offline,
  required String? projectId,
  required String text,
  required String? who,
  required bool dispatchOn,
  int files = 0,
  bool filesOn = false,
}) {
  if (offline) return 'Not connected to the server.';
  if (projectId == null) return 'Choose a project.';
  if (text.trim().isEmpty) return 'Describe what needs doing.';
  if (who == null || (who == dispatcherWho && !dispatchOn)) return 'Choose who does it.';
  if (files > 0 && !filesOn) return 'Add files in the session once it has started.';
  return null;
}

/// Everything a session start sends but the employee. `default` for model,
/// effort or mode means the employee's own.
class SessionStartInput {
  const SessionStartInput({
    required this.projectId,
    required this.text,
    this.model = 'default',
    this.effort = 'default',
    this.mode = 'default',
    this.images = const [],
    this.files = const [],
  });

  final String projectId;
  final String text;
  final String model;
  final String effort;
  final String mode;

  /// Uploaded photos (their server paths).
  final List<String> images;

  /// Uploaded files (their server paths); only on a server with `chat_files`.
  final List<String> files;
}

String? _pick(String value) => value == 'default' ? null : value;

/// Mode, model and effort it starts with; null when all are the employee's own.
TaskRun? _run(SessionStartInput start) {
  final mode = _pick(start.mode);
  final model = _pick(start.model);
  final effort = _pick(start.effort);
  if (mode == null && model == null && effort == null) return null;
  return TaskRun(mode: mode == null ? null : AgentMode.fromJson(mode), model: model, effort: effort);
}

/// `work_session.start`: the employee, the project and the first message. The
/// mobile app starts in the project's folder (no worktree choice yet).
StartWorkSession sessionStartParams(SessionStartInput start, {required String employeeId, required bool confirmSecret}) => StartWorkSession(
      projectId: start.projectId,
      employeeId: employeeId,
      text: start.text.trim(),
      images: start.images,
      files: start.files,
      worktree: false,
      run: _run(start),
      confirmSecret: confirmSecret,
    );

/// `work_session.dispatch`: a session start without the employee; the dispatcher chooses.
DispatchWorkSession sessionDispatchParams(SessionStartInput start, {required bool confirmSecret}) => DispatchWorkSession(
      projectId: start.projectId,
      text: start.text.trim(),
      images: start.images,
      files: start.files,
      worktree: false,
      run: _run(start),
      confirmSecret: confirmSecret,
    );

/// The session a start made, from its receipt (also one the dispatcher still gives to someone).
String? startedSession(Receipt receipt) => switch (receipt.result) {
      WriteResultCommand(value: CommandResultWorkSessionStarted(:final conversationId)) => conversationId,
      WriteResultCommand(value: CommandResultWorkSessionDispatching(:final conversationId)) => conversationId,
      _ => null,
    };
