import '../../data/discussion.dart' show mainTaskEnded;
import '../../data/run_config.dart' show ModelCatalog;
import '../../data/views.dart';

// A ticket as a session (`/ticket/:tid`, ticket #71, after the desktop's
// ticket-session-logic.ts of ticket #70): what sits above the input for its
// column, whether it only reads, who answers a message without a name and
// where an old link lands. The server decides all of it as before; these only
// read what it sends.

/// What sits right above the input: the decision of the moment.
enum TicketGate {
  /// Plan review: Approve, Approve, accept edits, Request changes, Reject.
  plan,

  /// Review: the report, Done (or Approve and merge) and Send back to Doing.
  review,

  /// Every other open column: the status box, which may have nothing to say.
  status,

  /// Done or archived: no gate (BR5).
  none,
}

/// Done or archived: the session only reads.
bool _closed(TicketView t) => t.column == ColumnKind.done || t.archivedAtMs != null;

/// The gate of [t]'s column (TS-04).
TicketGate ticketGate(TicketView t) {
  if (_closed(t)) return TicketGate.none;
  return switch (t.column) {
    ColumnKind.planReview => TicketGate.plan,
    ColumnKind.review => TicketGate.review,
    _ => TicketGate.status,
  };
}

/// The read-only bar of a Done or archived ticket ("#70 is Done · read-only.",
/// the desktop's words); null while it can be written. Leaving Done makes it
/// writable again with the next read of the ticket (TS-07.3).
String? sessionReadOnly(TicketView t) {
  if (!_closed(t)) return null;
  return '#${t.number} is ${t.column == ColumnKind.done ? 'Done' : 'archived'} · read-only.';
}

/// The columns where a message without a name goes to the owner (BR1).
const _ownerColumns = {ColumnKind.todo, ColumnKind.doing, ColumnKind.planReview, ColumnKind.testing, ColumnKind.review};

/// Who answers a message without an @Name (BR1, the server's `owner_answers`):
/// the owner while there is one, the ticket is not archived and its column is
/// Todo, Doing, Plan review, Testing or Review; null: nobody. Nobody either
/// while its main task ended (R1 m1): the server then answers "Nobody was
/// woken". [owner] is the owner's name from the company when the ticket has only its id.
String? ownerAnswers(TicketView t, {String? owner}) {
  if (t.ownerEmployeeId == null || t.archivedAtMs != null || !_ownerColumns.contains(t.column) || mainTaskEnded(t)) return null;
  return owner ?? t.ownerName ?? 'the owner';
}

/// The input's placeholder: "Message Pixel (owner)…", or the ticket when
/// nobody answers without a name. Shorter than the desktop's: on a phone a
/// longer one wraps the input; the hint under it says what @Name does.
String sessionPlaceholder(TicketView t, {String? owner}) {
  final to = ownerAnswers(t, owner: owner);
  return to == null ? 'Message #${t.number}…' : 'Message $to (owner)…';
}

/// The line under the input that says what a name does (as the desktop's hint).
String sessionHint(TicketView t, {String? owner}) {
  final to = ownerAnswers(t, owner: owner);
  return '@Name asks that employee${to == null ? '; without a name, no one is woken' : '; without a name, $to (owner) answers'}.';
}

/// The head's second line: "owner Forge · responsible You · Aurelius"; parts
/// that are empty are left out, "No owner" when there is none.
String headLine(TicketView t, {String? owner, String? me}) {
  final name = t.ownerName ?? owner;
  final responsible = t.responsibleUserId != null && t.responsibleUserId == me
      ? 'responsible You'
      : t.responsibleUsername == null
          ? null
          : 'responsible ${t.responsibleUsername}';
  return [
    t.ownerEmployeeId == null && name == null ? 'No owner' : 'owner ${name ?? 'unknown'}',
    ?responsible,
    if (t.projectName.isNotEmpty) t.projectName,
  ].join(' · ');
}

/// Where an old `?tab=` lands (BR6): the Activity opens its sheet; the old
/// Discussion and Ticket tabs are the session itself.
enum TicketLanding { session, activity }

/// [tab] of an old link to its place in the session.
TicketLanding sessionLanding(String? tab) => tab == 'activity' ? TicketLanding.activity : TicketLanding.session;

/// The ticket number in a route (`/ticket/70` or `/ticket/#70`, as the
/// desktop's links); null for an id, which `ticket.get` reads directly.
int? routeNumber(String tid) {
  final m = RegExp(r'^#?(\d+)$').firstMatch(tid.trim());
  return m == null ? null : int.tryParse(m.group(1)!);
}

/// Doing, and the main task stopped and waits for input: the status box
/// says so and offers Try again when its last answer failed.
bool mainTaskStopped(TicketView t, TaskView? root) => t.column == ColumnKind.doing && root != null && root.status == TaskStatus.waiting && root.waitingReason == 'input';

/// The last answer's failure in [messages], without the server's "send again"; null when it did not fail.
String? lastFailure(List<MessageView> messages) {
  final last = messages.where((m) => m.role == MessageRole.assistant && m.deletedAtMs == null).lastOrNull;
  if (last == null || last.status != MessageStatus.failed) return null;
  return last.text.replaceFirst(RegExp(r'\s*Send the message again to retry\.\s*$'), '');
}

/// What Try again sends to the owner: its last question again, as the desktop.
String retryText(List<MessageView> messages) => messages.where((m) => m.role == MessageRole.user).lastOrNull?.text ?? 'Please try again.';

/// The test task in Testing: the tester's latest task of the ticket; null without a tester or one.
TaskView? testTaskOf(List<TaskView> tasks, {required String? testerId}) {
  if (testerId == null) return null;
  TaskView? best;
  for (final t in tasks) {
    if (t.employeeId == testerId && (best == null || t.updatedAtMs >= best.updatedAtMs)) best = t;
  }
  return best;
}

/// Which run chips show under the input: none while the main task runs (as
/// the desktop's `runLocked`), and no Mode for a quick task.
({bool mode, bool model}) runChips(TicketView t, {required bool quick}) {
  final locked = t.rootTaskStatus == TaskStatus.running;
  return (mode: !locked && !quick, model: !locked);
}

/// The Mode chip: the ticket's mode, else [fallback] ("Your default · Ask";
/// "Default" on an older server).
String modeChipLabel(TaskRun run, [String fallback = 'Default']) => run.mode?.label ?? fallback;

/// The Model chip: the friendly name, else [fallback] ("Your default · Opus 5.5").
String modelChipLabel(TaskRun run, [String fallback = 'Default']) => run.model == null ? fallback : ModelCatalog.friendly(run.model!);

/// The Effort chip: the chosen level, else [fallback] ("Your default · High").
String effortChipLabel(TaskRun run, String fallback) => run.effort == null ? fallback : ModelCatalog.effortLabel(run.effort);

/// The models the Model chip offers: every connection's, once each, in order.
List<String> modelChoices(List<List<String>> perConnection) {
  final out = <String>[];
  for (final models in perConnection) {
    for (final m in models) {
      if (!out.contains(m)) out.add(m);
    }
  }
  return out;
}
