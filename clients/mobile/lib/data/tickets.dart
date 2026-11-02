import 'package:intl/intl.dart';

import 'protocol/protocol.dart';
import 'views.dart' show TaskViewState;

// The Board seen from the phone (ui-ontwerp §4–§6): which moves a board member
// may make between columns, the words for priorities, columns and the card's
// state, and what needs you. Derived from the generated protocol types only.

/// The server has the Testing column and the company tester (ticket #5,
/// protocol 5): only then does a client show either.
const boardTestingFeature = 'board_testing';

/// Why a hand cannot move a card into Testing or out of it, other than back to
/// Backlog (`domain::ticket::TESTING_MOVES_BY_ITSELF`).
const testingMovesByItself = 'Testing moves on by itself when the tests have run.';

/// The server keeps a board member responsible for a ticket (phase A, A1): only
/// then does a client show or send it, and only then can a ◆ belong to one person.
const ticketResponsibleFeature = 'ticket_responsible';

/// The server keeps who follows a ticket (phase A, A2).
const ticketFollowFeature = 'ticket_follow';

/// [board] as this client shows it: without the Testing column when the server
/// lacks [boardTestingFeature] ([testing] false).
BoardView boardShown(BoardView board, {required bool testing}) => testing
    ? board
    : BoardView(asOfSeq: board.asOfSeq, projectId: board.projectId, columns: [for (final c in board.columns) if (c.column.kind != ColumnKind.testing) c]);

/// What a move between two columns does once allowed (§6).
enum MoveKind {
  /// Within its column: only the order.
  reorder,

  /// Backlog → Todo: Start, at once (MU1-09).
  start,

  /// To Backlog: stops the work (a confirmation where work runs).
  cancel,

  /// Plan review → Doing: opens the plan review.
  reviewPlan,

  /// Review → Doing: opens "Send back to Doing" with required feedback.
  sendBack,

  /// Review → Done: opens "Mark as Done?".
  accept,
}

/// Whether a board member may move a card from one column to another, and why not.
sealed class MoveRule {
  const MoveRule();
}

class MoveAllowed extends MoveRule {
  const MoveAllowed(this.kind, {this.confirm});

  final MoveKind kind;

  /// Asked first (a "Back to Backlog" that stops work); null when it just happens.
  final String? confirm;
}

class MoveRefused extends MoveRule {
  const MoveRefused(this.reason);

  /// One line in words, never just a stop sign (§6).
  final String reason;
}

/// The table of §6 for a board member. Slepen (tablet), "Move to…" and the
/// buttons all follow it; the server checks it again.
MoveRule moveRule(ColumnKind from, ColumnKind to) {
  if (from == ColumnKind.done) return to == ColumnKind.done ? const MoveRefused('Done tickets are sorted by when they closed.') : const MoveRefused('Done tickets are closed.');
  if (from == to) return const MoveAllowed(MoveKind.reorder);
  return switch ((from, to)) {
    // The tester moves it on (ticket #5); a hand only cancels it.
    (ColumnKind.testing, ColumnKind.backlog) => const MoveAllowed(MoveKind.cancel, confirm: 'This stops the tests and the work on the ticket.'),
    (ColumnKind.testing, ColumnKind.done) => const MoveRefused('Only from Review.'),
    (ColumnKind.testing, _) || (_, ColumnKind.testing) => const MoveRefused(testingMovesByItself),
    (ColumnKind.backlog, ColumnKind.todo) => const MoveAllowed(MoveKind.start),
    (ColumnKind.backlog, _) => const MoveRefused('Start first.'),
    (ColumnKind.todo, ColumnKind.backlog) => const MoveAllowed(MoveKind.cancel, confirm: 'The dispatcher stops choosing an owner.'),
    (ColumnKind.todo, ColumnKind.doing) => const MoveRefused('Doing starts when an owner is chosen.'),
    (ColumnKind.todo, _) => const MoveRefused('Doing starts when an owner is chosen.'),
    (ColumnKind.doing, ColumnKind.backlog) => const MoveAllowed(MoveKind.cancel, confirm: 'This stops the main task and its delegations.'),
    (ColumnKind.doing, ColumnKind.todo) => const MoveRefused('Not from Doing.'),
    (ColumnKind.doing, ColumnKind.planReview) => const MoveRefused('The engine moves tickets here.'),
    (ColumnKind.doing, ColumnKind.review) => const MoveRefused('The owner hands it in.'),
    (ColumnKind.doing, _) => const MoveRefused('Only from Review.'),
    (ColumnKind.planReview, ColumnKind.backlog) => const MoveAllowed(MoveKind.cancel, confirm: 'This rejects the plan and stops the work.'),
    (ColumnKind.planReview, ColumnKind.doing) => const MoveAllowed(MoveKind.reviewPlan),
    (ColumnKind.planReview, _) => const MoveRefused('Approve or revise the plan first.'),
    (ColumnKind.review, ColumnKind.backlog) => const MoveAllowed(MoveKind.cancel, confirm: 'This stops the work on the ticket.'),
    (ColumnKind.review, ColumnKind.doing) => const MoveAllowed(MoveKind.sendBack),
    (ColumnKind.review, ColumnKind.done) => const MoveAllowed(MoveKind.accept),
    (ColumnKind.review, _) => const MoveRefused('Send it back or mark it as Done.'),
    _ => const MoveRefused('Not allowed.'),
  };
}

/// Where a card lands relative to its neighbours: `before_ticket_id` of
/// `ticket.move` (null: at the bottom). [cards] is the target column in order;
/// [index] is where the card goes once it is out of its old place.
String? beforeTicketAt(List<TicketView> cards, String ticketId, int index) {
  final others = [for (final c in cards) if (c.id != ticketId) c];
  return index < others.length ? others[index < 0 ? 0 : index].id : null;
}

extension ColumnKindView on ColumnKind {
  /// The fixed role of a column, shown beside its (renamable) name (§12.1).
  String get role => switch (this) {
        ColumnKind.backlog => 'Start',
        ColumnKind.todo => 'Dispatch',
        ColumnKind.doing => 'Work',
        ColumnKind.planReview => 'Plan gate',
        ColumnKind.testing => 'Test gate',
        ColumnKind.review => 'Review gate',
        ColumnKind.done => 'Closed',
      };

  /// One line in an empty column (§3.3).
  String get emptyText => switch (this) {
        ColumnKind.backlog => 'New tickets land here.',
        ColumnKind.todo => 'Started tickets wait here for an owner.',
        ColumnKind.doing => 'Nothing in progress.',
        ColumnKind.planReview => 'Only tickets in Plan mode stop here, with their plan.',
        ColumnKind.testing => 'Reported tickets are tested here once before Review.',
        ColumnKind.review => 'Nothing to review.',
        ColumnKind.done => 'Closed tickets appear here.',
      };
}

/// The order of priorities in pickers (B9).
const ticketPriorities = [TicketPriority.low, TicketPriority.normal, TicketPriority.high, TicketPriority.urgent];

extension TicketPriorityView on TicketPriority {
  String get label => switch (this) {
        TicketPriority.low => 'Low',
        TicketPriority.normal => 'Normal',
        TicketPriority.high => 'High',
        TicketPriority.urgent => 'Urgent',
      };

  /// Shape and word, never colour alone (§5.1, §15); Normal shows nothing on a card.
  String? get mark => switch (this) {
        TicketPriority.urgent => '▲▲ Urgent',
        TicketPriority.high => '▲ High',
        TicketPriority.low => '▽ Low',
        TicketPriority.normal => null,
      };
}

/// The employee chosen to dispatch [project]'s tickets, active or not: its own,
/// else the company's (`storage::company::dispatcher_for`, before the active check).
String? chosenDispatcher(ProjectSummary? project, String? companyDispatcher) => project?.dispatcherEmployeeId ?? companyDispatcher;

/// The employee who chooses owners for [project]'s tickets: the chosen one
/// ([chosenDispatcher]) when active; null when nobody does. B5 strict: a paused
/// or archived dispatcher of the project does not pass to the company's.
String? effectiveDispatcher(ProjectSummary? project, String? companyDispatcher, List<EmployeeSummary> employees) {
  final chosen = chosenDispatcher(project, companyDispatcher);
  return chosen != null && employees.any((e) => e.id == chosen && e.status == LifecycleStatus.active) ? chosen : null;
}

/// Whether the dispatcher gave up on a ticket in Todo (B5): the server says it
/// with a `ticket.updated` the system made (`needs: owner`), after the ticket's
/// last start or choice of an owner. [activity] is the ticket's timeline, newest first.
bool dispatcherGaveUp(List<TimelineCard> activity) {
  for (final card in activity) {
    switch (card.title) {
      case 'ticket.started' || 'ticket.dispatched' || 'ticket.cancelled':
        return false;
      case 'ticket.updated' when card.actor == 'system':
        return true;
    }
  }
  return false;
}

/// What a card or the ticket view says about where the ticket stands.
enum TicketState {
  /// Backlog: waits for Start.
  backlog,

  /// Todo: the dispatcher is choosing an owner.
  choosing,

  /// Todo: nobody chooses (no dispatcher, or it could not); a board member must (B5).
  needsOwner,

  /// Todo: whether the dispatcher still chooses could not be read (its
  /// activity failed); shown as needing you, never silently as choosing.
  ownerUnknown,

  /// Todo: an owner is chosen; the main task waits for its turn.
  queued,

  /// Doing: the main task runs or waits.
  working,

  /// Doing: the main task waits for a decision of yours (an approval, a question).
  waitingForYou,

  /// Doing: the main task failed; "Start again" (B6).
  failed,

  /// Todo/Doing: it waits for the Claude login of the board member it runs
  /// on (`needs: login`), and that is you: Connect / Reconnect (MU3-12, §5).
  needsYourLogin,

  /// Todo/Doing: it waits for another board member's Claude login; seen, not
  /// a Needs you (§3). "Run on my login" takes it over (B4).
  waitsForLogin,

  /// Todo/Doing: its runs wait for the usage limit of the login they run on
  /// (MU3-11); no action, only the time.
  waitingForLimit,

  /// Plan review: a plan waits.
  planReview,

  /// Testing: the tester runs the full test suite once (ticket #5); nothing for you.
  testing,

  /// Review: a report waits.
  review,

  done,
}

/// [ticket]'s state; [root] is its main task when known, [needsOwner] whether
/// nobody chooses its owner in Todo, [ownerUnknown] whether that could not be
/// read, [me] the signed-in board member (a wait for a login is theirs only).
TicketState ticketState(TicketView ticket, {TaskView? root, bool needsOwner = false, bool ownerUnknown = false, String? me}) => switch (ticket.column) {
      ColumnKind.backlog => TicketState.backlog,
      // A failed main task, in Todo too (as on the desktop): "Start again" (B6).
      ColumnKind.todo || ColumnKind.doing when (root?.status ?? ticket.rootTaskStatus) == TaskStatus.failed => TicketState.failed,
      // No fallback to another login (MU3-12): it waits until its own is there.
      ColumnKind.todo || ColumnKind.doing when ticket.needs == TicketNeed.login => ticket.loginUser == me ? TicketState.needsYourLogin : TicketState.waitsForLogin,
      ColumnKind.todo || ColumnKind.doing when root != null && root.status == TaskStatus.waiting && root.waitingReason == 'quota' => TicketState.waitingForLimit,
      ColumnKind.todo when ticket.ownerEmployeeId != null => TicketState.queued,
      ColumnKind.todo when needsOwner => TicketState.needsOwner,
      ColumnKind.todo => ownerUnknown ? TicketState.ownerUnknown : TicketState.choosing,
      ColumnKind.doing when root != null && root.needsYou => TicketState.waitingForYou,
      ColumnKind.doing => TicketState.working,
      ColumnKind.planReview => TicketState.planReview,
      ColumnKind.testing => TicketState.testing,
      ColumnKind.review => TicketState.review,
      ColumnKind.done => TicketState.done,
    };

extension TicketStateView on TicketState {
  /// Waits for a board member (§5.2): what the card and the page show, its
  /// button and status line. Whether the ◆ is yours: [needsMe].
  bool get needsYou => switch (this) {
        TicketState.needsOwner || TicketState.ownerUnknown || TicketState.waitingForYou || TicketState.needsYourLogin || TicketState.failed || TicketState.planReview || TicketState.review => true,
        _ => false,
      };

  /// The status line on a card; null where the column says enough. [login]:
  /// whose Claude login it runs on (`login_username`, "your" when it is
  /// yours), [untilMs] when its limit resets, [tester] who runs the tests,
  /// [waitsFor] who a decision is for when that is someone else ([needsMe]).
  String? line({String? owner, String? dispatcher, String? tester, String? login, bool mine = false, int? untilMs, bool expired = false, String? waitsFor}) => switch (this) {
        TicketState.backlog => null,
        TicketState.choosing => '${dispatcher ?? 'The dispatcher'} is choosing…',
        TicketState.needsOwner => 'Choose an employee',
        TicketState.ownerUnknown => 'Dispatcher unknown · choose an employee',
        TicketState.queued => 'Queued for ${owner ?? 'its owner'}',
        TicketState.working => 'Working',
        TicketState.waitingForYou => waitsFor == null ? 'Waiting for you' : 'Waiting for $waitsFor',
        TicketState.needsYourLogin => expired ? 'Waiting for your Claude login (expired)' : 'Waiting for your Claude login',
        TicketState.waitsForLogin => "Waiting for ${login ?? 'its creator'}'s Claude login",
        TicketState.waitingForLimit => limitWaitOf(mine ? 'your' : "${login ?? 'its creator'}'s", untilMs),
        TicketState.failed => 'Failed',
        TicketState.planReview => 'Plan to review',
        TicketState.testing => '${tester ?? 'The tester'} is testing…',
        TicketState.review => 'Report to review',
        TicketState.done => 'Done',
      };
}

/// Whether a ticket in [state] counts as [me]'s work (B6, freeze §1): its ◆, the
/// filter Needs you, the count and Waiting on me. A wait for a login is the login
/// user's ([TicketState.needsYourLogin] already asked); the rest the responsible's,
/// and everyone's without an active one, as on a server without a responsible.
/// Only what counts: anyone can still decide ([TicketState.needsYou]).
bool needsMe(TicketView ticket, TicketState state, {String? me}) {
  if (!state.needsYou) return false;
  if (state == TicketState.needsYourLogin) return true;
  final responsible = ticket.responsibleUserId;
  return responsible == null || (ticket.responsibleDisabled ?? false) || responsible == me;
}

/// Who a decision that is not [me]'s waits for (the responsible's name); null when it is [me]'s or nobody's.
String? waitsForOther(TicketView ticket, TicketState state, {String? me}) => state.needsYou && !needsMe(ticket, state, me: me) ? ticket.responsibleUsername : null;

/// What the Responsible is for, under the field and in its picker (P-71).
const responsibleExplained = 'Follows up and reviews. An employee does the work.';

/// The name of [ticket]'s responsible: "You", "Alexandra (disabled)"; null without one.
String? responsibleLabel(TicketView ticket, {String? me}) {
  final id = ticket.responsibleUserId;
  if (id == null) return null;
  if (id == me) return 'You';
  final name = ticket.responsibleUsername ?? 'A board member';
  return (ticket.responsibleDisabled ?? false) ? '$name (disabled)' : name;
}

/// Whose Claude login [ticket] runs on, for the card's "on … login" (P-73).
String loginOwnerOf(TicketView ticket, {String? me}) => ticket.loginUser == me ? 'your' : "${ticket.loginUsername}'s";

/// The whole who-block as one sentence, for a screen reader (§1.3, §9).
String whoSentence(TicketView ticket, {String? owner, String? me}) {
  final responsible = responsibleLabel(ticket, me: me);
  return [
    owner == null ? 'No owner yet.' : '$owner does the work.',
    'Runs on ${loginOwnerOf(ticket, me: me)} Claude login.',
    responsible == null ? 'No one responsible.' : '${responsible == 'You' ? 'You are' : '$responsible is'} responsible: follows up and reviews.',
    'Created by ${ticket.createdByUser == me ? 'you' : ticket.createdByUsername}.',
  ].join(' ');
}

/// Who [me] can make responsible (§2.1): [me] first as "You", then the other
/// active board members A–Z. A disabled one is no longer a choice (P-78).
List<({String id, String label})> responsibleChoices(List<UserView> members, {String? me}) {
  final others = [for (final u in members) if (u.disabledAtMs == null && u.id != me) u]..sort((a, b) => a.username.toLowerCase().compareTo(b.username.toLowerCase()));
  return [
    if (me != null) (id: me, label: 'You'),
    for (final u in others) (id: u.id, label: u.username),
  ];
}

/// How many open tickets on [board] have [userId] responsible: what disabling them leaves behind (§2.4).
int openResponsibleCount(BoardView board, String userId) => board.columns.where((c) => c.column.kind != ColumnKind.done).expand((c) => c.tickets).where((t) => t.responsibleUserId == userId).length;

/// Why a board member follows (`FollowerView.reason`, text, so an unknown one
/// from a newer server just shows nothing): in the followers list.
String? followReasonLabel(String reason) => switch (reason) {
      'created' => 'Created it',
      'responsible' => 'Responsible',
      'mentioned' => '@mentioned',
      'manual' => 'Chose to follow',
      _ => null,
    };

/// Whether [me] may stop following [ticket]: not while responsible for it (B10).
bool canUnfollow(TicketView ticket, {String? me}) => ticket.responsibleUserId == null || ticket.responsibleUserId != me;

/// The line under the switch Following (P-79, §3.2–§3.3): why, so following by
/// itself can be explained rather than surprise. [reason] is [me]'s, null when
/// not following.
String followingNote(TicketView ticket, {String? reason, String? me}) {
  if (!canUnfollow(ticket, me: me)) return "You're responsible for #${ticket.number}. Hand it to someone else to stop following.";
  if (ticket.column == ColumnKind.done || ticket.archivedAtMs != null) return 'Done tickets send no updates.';
  return switch (reason) {
    null => 'Get updates when it needs review, is sent back or fails.',
    'created' => 'You follow because you created it.',
    'responsible' => "You follow because you're responsible.",
    'mentioned' => 'You follow because you were @mentioned.',
    'manual' => 'You follow because you chose to.',
    _ => 'You follow this ticket.',
  };
}

/// "Waiting for Robin's limit until 16:00" (P-37: the name and the time, never
/// someone else's usage), with the day when that is not today.
String limitWaitOf(String whose, int? untilMs, {DateTime? now}) {
  if (untilMs == null) return 'Waiting for $whose limit to reset';
  final at = DateTime.fromMillisecondsSinceEpoch(untilMs);
  final n = now ?? DateTime.now();
  final today = at.year == n.year && at.month == n.month && at.day == n.day;
  return 'Waiting for $whose limit until ${DateFormat(today ? 'HH:mm' : 'EEE d MMM HH:mm').format(at)}';
}

/// "Due Nov 3", "Due today 17:00", "Overdue · Oct 29" (§5.1); null without a deadline.
/// [warn] is true when it is due within a day or past, never in Done.
({String text, bool late, bool soon})? deadlineOf(TicketView ticket, {DateTime? now}) {
  final ms = ticket.deadlineMs;
  if (ms == null) return null;
  final at = DateTime.fromMillisecondsSinceEpoch(ms);
  final today = now ?? DateTime.now();
  final done = ticket.column == ColumnKind.done;
  if (!done && at.isBefore(today)) return (text: 'Overdue · ${DateFormat.MMMd().format(at)}', late: true, soon: false);
  final sameDay = at.year == today.year && at.month == today.month && at.day == today.day;
  if (!done && sameDay) return (text: 'Due today ${DateFormat.Hm().format(at)}', late: false, soon: true);
  final soon = !done && at.difference(today) < const Duration(hours: 24);
  return (text: 'Due ${DateFormat.MMMd().format(at)}', late: false, soon: soon);
}

/// The tasks of a ticket as a tree in reading order: the main tasks oldest
/// first, each followed by its delegations (depth 1, 2, …).
List<(TaskView, int)> ticketTaskTree(List<TaskView> tasks) {
  final ids = {for (final t in tasks) t.id};
  final byParent = <String?, List<TaskView>>{};
  for (final t in tasks) {
    byParent.putIfAbsent(ids.contains(t.parentTaskId) ? t.parentTaskId : null, () => []).add(t);
  }
  final out = <(TaskView, int)>[];
  final seen = <String>{};
  void walk(TaskView t, int depth) {
    if (!seen.add(t.id)) return;
    out.add((t, depth));
    for (final c in byParent[t.id] ?? const <TaskView>[]) {
      walk(c, depth + 1);
    }
  }

  for (final t in byParent[null] ?? const <TaskView>[]) {
    walk(t, 0);
  }
  return out;
}

/// The board's version for `board.columns_update`: the highest column version (S3d).
int boardVersion(BoardView board) => board.columns.fold(0, (v, c) => c.column.version > v ? c.column.version : v);

/// What an entry of the ticket's activity says (§7.4), from its event type.
String activityText(TimelineCard card) {
  final who = switch (card.actor) {
    'user' => 'A board member',
    'employee' => 'The owner',
    _ => 'Aurelius',
  };
  return switch (card.title) {
    'ticket.created' => '$who created the ticket',
    'ticket.started' => '$who started it',
    'ticket.dispatched' => card.actor == 'employee' ? 'The dispatcher chose an owner' : '$who chose an owner',
    'ticket.moved' => '$who moved it',
    'ticket.plan_review' => 'A plan was handed in',
    'ticket.plan_approved' => '$who approved the plan',
    'ticket.plan_revised' => '$who sent the plan back',
    'ticket.review' => 'A report was handed in',
    'ticket.tested' => switch (card.detail) {
        'passed' => 'Tests passed',
        'failed' => 'Tests failed',
        _ => 'The tests ran',
      },
    'ticket.sent_back' => '$who sent it back to Doing',
    'ticket.accepted' => '$who marked it as Done',
    'ticket.cancelled' => '$who put it back in Backlog',
    'ticket.archived' => '$who archived it',
    'ticket.updated' when card.actor == 'system' => 'Nobody chose an owner; a board member must',
    'ticket.updated' => '$who changed it',
    _ => card.title,
  };
}
