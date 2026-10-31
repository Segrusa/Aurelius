import 'package:aurelius_mobile/data/tickets.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart' show parseLabels;
import 'package:flutter_test/flutter_test.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

void main() {
  group('the move rules of ui-ontwerp §6', () {
    MoveKind? allowed(ColumnKind from, ColumnKind to) => switch (moveRule(from, to)) { MoveAllowed(:final kind) => kind, MoveRefused() => null };

    test('Backlog → Todo is Start; nothing else leaves Backlog (MU1-09)', () {
      expect(allowed(ColumnKind.backlog, ColumnKind.todo), MoveKind.start);
      for (final to in [ColumnKind.doing, ColumnKind.planReview, ColumnKind.review, ColumnKind.done]) {
        expect(moveRule(ColumnKind.backlog, to), isA<MoveRefused>().having((r) => r.reason, 'reason', 'Start first.'));
      }
    });

    test('back to Backlog stops the work, with a confirmation from Todo, Doing, Plan review and Review', () {
      for (final from in [ColumnKind.todo, ColumnKind.doing, ColumnKind.planReview, ColumnKind.review]) {
        final rule = moveRule(from, ColumnKind.backlog);
        expect(rule, isA<MoveAllowed>().having((r) => r.kind, 'kind', MoveKind.cancel).having((r) => r.confirm, 'confirm', isNotNull), reason: from.wire);
      }
    });

    test('the engine and the owner move a card forward, a board member does not', () {
      expect(moveRule(ColumnKind.todo, ColumnKind.doing), isA<MoveRefused>());
      expect(moveRule(ColumnKind.doing, ColumnKind.todo), isA<MoveRefused>().having((r) => r.reason, 'reason', 'Not from Doing.'));
      expect(moveRule(ColumnKind.doing, ColumnKind.planReview), isA<MoveRefused>().having((r) => r.reason, 'reason', 'The engine moves tickets here.'));
      expect(moveRule(ColumnKind.doing, ColumnKind.review), isA<MoveRefused>().having((r) => r.reason, 'reason', 'The owner hands it in.'));
      expect(moveRule(ColumnKind.doing, ColumnKind.done), isA<MoveRefused>().having((r) => r.reason, 'reason', 'Only from Review.'));
    });

    test('Plan review → Doing opens the plan; Review → Doing asks feedback; Review → Done asks to confirm (MU1-13, MU1-16)', () {
      expect(allowed(ColumnKind.planReview, ColumnKind.doing), MoveKind.reviewPlan);
      expect(allowed(ColumnKind.review, ColumnKind.doing), MoveKind.sendBack);
      expect(allowed(ColumnKind.review, ColumnKind.done), MoveKind.accept);
      expect(moveRule(ColumnKind.planReview, ColumnKind.review), isA<MoveRefused>());
    });

    test('Done is closed, also for the order; elsewhere the order may change', () {
      for (final to in ColumnKind.values) {
        expect(moveRule(ColumnKind.done, to), isA<MoveRefused>(), reason: to.wire);
      }
      for (final kind in ColumnKind.values.where((k) => k != ColumnKind.done)) {
        expect(allowed(kind, kind), MoveKind.reorder);
      }
    });

    test('every refusal has a reason in words', () {
      for (final from in ColumnKind.values) {
        for (final to in ColumnKind.values) {
          if (moveRule(from, to) case MoveRefused(:final reason)) expect(reason, isNotEmpty);
        }
      }
    });
  });

  test('before_ticket_id: the card after the new place, without the moved card itself; none at the bottom', () {
    final cards = [ticketOf(id: 'a'), ticketOf(id: 'b'), ticketOf(id: 'c')];
    expect(beforeTicketAt(cards, 'a', 1), 'c', reason: 'a moves down one: above c');
    expect(beforeTicketAt(cards, 'c', 0), 'a', reason: 'c to the top');
    expect(beforeTicketAt(cards, 'a', 2), isNull, reason: 'to the bottom');
    expect(beforeTicketAt(cards, 'x', 3), isNull);
  });

  test('priorities show a shape and a word; Normal shows nothing on a card (B9, §15)', () {
    expect(ticketPriorities.map((p) => p.label), ['Low', 'Normal', 'High', 'Urgent']);
    expect(TicketPriority.urgent.mark, '▲▲ Urgent');
    expect(TicketPriority.high.mark, '▲ High');
    expect(TicketPriority.low.mark, '▽ Low');
    expect(TicketPriority.normal.mark, isNull);
  });

  group('the dispatcher', () {
    final archived = employeeOf(id: 'gone', name: 'Gone', status: LifecycleStatus.archived);
    final paused = employeeOf(id: 'resting', name: 'Resting', status: LifecycleStatus.paused);
    final employees = [orion, forge, archived, paused];
    ProjectSummary project(String? dispatcher) => ProjectSummary(createdAtMs: 0, id: 'p1', name: 'Atlas', status: ProjectStatus.active, version: 1, employeesActive: 0, dispatcherEmployeeId: dispatcher);

    test("a project's own dispatcher wins; without one the company's (MU1-11 AC1/AC2)", () {
      expect(effectiveDispatcher(project('forge'), 'orion', employees), 'forge');
      expect(effectiveDispatcher(project(null), 'orion', employees), 'orion');
    });

    test("B5 strict: a paused or archived project dispatcher does not pass to the company's (as dispatcher_for)", () {
      expect(effectiveDispatcher(project('gone'), 'orion', employees), isNull, reason: 'archived: nobody chooses, no fallback');
      expect(effectiveDispatcher(project('resting'), 'orion', employees), isNull, reason: 'paused: nobody chooses, no fallback');
      expect(effectiveDispatcher(project(null), 'gone', employees), isNull, reason: "the company's own, archived");
      expect(effectiveDispatcher(project(null), 'resting', employees), isNull, reason: "the company's own, paused");
      expect(effectiveDispatcher(project('unknown'), 'orion', employees), isNull, reason: 'not an employee: not active');
      expect(effectiveDispatcher(project(null), null, employees), isNull);
      expect(chosenDispatcher(project('gone'), 'orion'), 'gone', reason: 'project ?? company, before the active check');
      expect(chosenDispatcher(project(null), 'orion'), 'orion');
    });

    test('it gave up when the system said so after the last start (ticket.updated needs owner)', () {
      expect(dispatcherGaveUp([cardOf('ticket.updated', actor: 'system', seq: 3), cardOf('ticket.started', seq: 2)]), isTrue);
      expect(dispatcherGaveUp([cardOf('ticket.started', seq: 4), cardOf('ticket.updated', actor: 'system', seq: 3)]), isFalse, reason: 'started again since');
      expect(dispatcherGaveUp([cardOf('ticket.updated', actor: 'user', seq: 3), cardOf('ticket.started', seq: 2)]), isFalse, reason: 'a board member edited it');
      expect(dispatcherGaveUp(const []), isFalse);
    });
  });

  group('where a ticket stands', () {
    test('Todo: choosing, needs an owner, or queued for its owner', () {
      expect(ticketState(ticketOf(column: ColumnKind.todo)), TicketState.choosing);
      expect(ticketState(ticketOf(column: ColumnKind.todo), needsOwner: true), TicketState.needsOwner);
      expect(ticketState(ticketOf(column: ColumnKind.todo, ownerEmployeeId: 'forge'), needsOwner: true), TicketState.queued);
    });

    test('Doing: working, waiting for you, or failed (B6)', () {
      expect(ticketState(ticketOf(column: ColumnKind.doing), root: taskOf(status: TaskStatus.running)), TicketState.working);
      expect(ticketState(ticketOf(column: ColumnKind.doing), root: taskOf(status: TaskStatus.running, pendingApprovals: 1)), TicketState.waitingForYou);
      expect(ticketState(ticketOf(column: ColumnKind.doing, rootTaskStatus: TaskStatus.failed)), TicketState.failed);
    });

    test('Todo with a failed main task is failed too, as on the desktop (with or without an owner)', () {
      expect(ticketState(ticketOf(column: ColumnKind.todo, rootTaskStatus: TaskStatus.failed)), TicketState.failed);
      expect(ticketState(ticketOf(column: ColumnKind.todo, ownerEmployeeId: 'forge'), root: taskOf(status: TaskStatus.failed)), TicketState.failed);
      expect(ticketState(ticketOf(column: ColumnKind.todo, rootTaskStatus: TaskStatus.failed), needsOwner: true), TicketState.failed);
    });

    test('Needs you: the gates, a failure, a question, nobody to choose an owner (§5.2), your own Claude login (Fase 3 §3)', () {
      expect([for (final s in TicketState.values) if (s.needsYou) s], [TicketState.needsOwner, TicketState.ownerUnknown, TicketState.waitingForYou, TicketState.failed, TicketState.needsYourLogin, TicketState.planReview, TicketState.review]);
      expect(TicketState.choosing.line(dispatcher: 'Orion'), 'Orion is choosing…');
      expect(TicketState.queued.line(owner: 'Forge'), 'Queued for Forge');
    });
  });

  test('deadlines: due, due today, overdue; never late in Done (§5.1)', () {
    final now = DateTime(2026, 11, 3, 9);
    expect(deadlineOf(ticketOf(deadlineMs: DateTime(2026, 11, 7, 17).millisecondsSinceEpoch), now: now)!.text, 'Due Nov 7');
    final today = deadlineOf(ticketOf(deadlineMs: DateTime(2026, 11, 3, 17).millisecondsSinceEpoch), now: now)!;
    expect((today.text, today.soon), ('Due today 17:00', true));
    final late = deadlineOf(ticketOf(deadlineMs: DateTime(2026, 10, 29, 17).millisecondsSinceEpoch), now: now)!;
    expect((late.text, late.late), ('Overdue · Oct 29', true));
    final closed = deadlineOf(ticketOf(column: ColumnKind.done, deadlineMs: DateTime(2026, 10, 29, 17).millisecondsSinceEpoch), now: now)!;
    expect(closed.late, isFalse);
    expect(deadlineOf(ticketOf()), isNull);
  });

  test("a ticket's tasks as a tree: the main task, then its delegations under it (MU1-08 AC2)", () {
    final tree = ticketTaskTree([
      taskOf(id: 'root', title: 'Main'),
      taskOf(id: 'd1', title: 'Users table', parentTaskId: 'root'),
      taskOf(id: 'd2', title: 'Review params', parentTaskId: 'd1'),
      taskOf(id: 'root2', title: 'Second main'),
    ]);
    expect([for (final (t, depth) in tree) '${'  ' * depth}${t.id}'], ['root', '  d1', '    d2', 'root2']);
  });

  test("the board's version is its highest column version (S3d)", () {
    expect(boardVersion(boardOf(const [], versions: {ColumnKind.review: 7, ColumnKind.todo: 3})), 7);
  });

  test('labels: split on commas, trimmed, each once', () {
    expect(parseLabels(' backend, security ,, backend,ui '), ['backend', 'security', 'ui']);
    expect(parseLabels(''), isEmpty);
  });

  test('activity lines say who did what (MU1-15 AC4)', () {
    expect(activityText(cardOf('ticket.started')), 'A board member started it');
    expect(activityText(cardOf('ticket.review', actor: 'employee')), 'A report was handed in');
    expect(activityText(cardOf('ticket.dispatched', actor: 'employee')), 'The dispatcher chose an owner');
    expect(activityText(cardOf('ticket.updated', actor: 'system')), 'Nobody chose an owner; a board member must');
    expect(activityText(cardOf('ticket.accepted')), 'A board member marked it as Done');
  });

  test("a ticket's main task is not assigned or cancelled as a task; a delegation under it is", () {
    final main = taskOf(status: TaskStatus.queued, ticketId: 'k1');
    final delegation = taskOf(status: TaskStatus.queued, ticketId: 'k1', parentTaskId: 'root');
    expect(main.isTicketMain, isTrue);
    expect(main.canAssign, isFalse);
    expect(delegation.isTicketMain, isFalse);
    expect(delegation.canAssign, isTrue);
    expect(taskOf(status: TaskStatus.queued).isTicketMain, isFalse);
  });
}
