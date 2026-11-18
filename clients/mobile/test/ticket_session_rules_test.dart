import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/ticket_session_rules.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

void main() {
  group('the gate above the input (TS-04)', () {
    test('Plan review has the plan gate, Review the review gate', () {
      expect(ticketGate(ticketOf(column: ColumnKind.planReview)), TicketGate.plan);
      expect(ticketGate(ticketOf(column: ColumnKind.review)), TicketGate.review);
    });

    test('every other open column has the status box', () {
      for (final c in [ColumnKind.backlog, ColumnKind.todo, ColumnKind.doing, ColumnKind.testing]) {
        expect(ticketGate(ticketOf(column: c)), TicketGate.status, reason: c.wire);
      }
    });

    test('Done and archived have no gate (BR5)', () {
      expect(ticketGate(ticketOf(column: ColumnKind.done)), TicketGate.none);
      expect(ticketGate(ticketOf(column: ColumnKind.review, archivedAtMs: 5)), TicketGate.none);
      expect(ticketGate(ticketOf(column: ColumnKind.backlog, archivedAtMs: 5)), TicketGate.none);
    });
  });

  group('read-only (BR5, TS-07)', () {
    test('Done or archived is read-only, with the bar of the desktop', () {
      expect(sessionReadOnly(ticketOf(number: 70, column: ColumnKind.done)), '#70 is Done · read-only.');
      expect(sessionReadOnly(ticketOf(number: 70, column: ColumnKind.backlog, archivedAtMs: 1)), '#70 is archived · read-only.');
      expect(sessionReadOnly(ticketOf(number: 70, column: ColumnKind.done, archivedAtMs: 1)), '#70 is Done · read-only.', reason: 'Done says more than archived');
    });

    test('an open ticket can be written, also after it left Done', () {
      for (final c in ColumnKind.values.where((c) => c != ColumnKind.done)) {
        expect(sessionReadOnly(ticketOf(column: c)), isNull, reason: c.wire);
      }
    });
  });

  group('who answers a message without a name (BR1)', () {
    test('the owner in Todo, Doing, Plan review, Testing and Review', () {
      for (final c in [ColumnKind.todo, ColumnKind.doing, ColumnKind.planReview, ColumnKind.testing, ColumnKind.review]) {
        expect(ownerAnswers(ticketOf(column: c, ownerEmployeeId: 'forge', ownerName: 'Forge')), 'Forge', reason: c.wire);
      }
    });

    test('nobody in Backlog or Done, when archived or without an owner', () {
      expect(ownerAnswers(ticketOf(column: ColumnKind.backlog, ownerEmployeeId: 'forge', ownerName: 'Forge')), isNull);
      expect(ownerAnswers(ticketOf(column: ColumnKind.done, ownerEmployeeId: 'forge', ownerName: 'Forge')), isNull);
      expect(ownerAnswers(ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge', archivedAtMs: 1)), isNull);
      expect(ownerAnswers(ticketOf(column: ColumnKind.todo)), isNull);
    });

    test("the owner's name from the company when the ticket has only the id", () {
      expect(ownerAnswers(ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge'), owner: 'Forge'), 'Forge');
      expect(ownerAnswers(ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge')), 'the owner');
    });

    test('the placeholder and the hint say who answers, as the desktop', () {
      final owned = ticketOf(number: 70, column: ColumnKind.doing, ownerEmployeeId: 'pixel', ownerName: 'Pixel');
      expect(sessionPlaceholder(owned), 'Message Pixel (owner)… · @ to mention');
      expect(sessionHint(owned), '@Name asks that employee; without a name, Pixel (owner) answers.');
      final nobody = ticketOf(number: 70, column: ColumnKind.backlog, ownerEmployeeId: 'pixel', ownerName: 'Pixel');
      expect(sessionPlaceholder(nobody), 'Message #70 · @ to mention');
      expect(sessionHint(nobody), '@Name asks that employee; without a name, no one is woken.');
    });
  });

  group('the head (TS-05.1)', () {
    test('owner, responsible and project; "You" when it is me', () {
      final t = ticketOf(ownerEmployeeId: 'forge', ownerName: 'Forge', responsible: bo, projectName: 'Aurelius');
      expect(headLine(t, me: 'u-1'), 'owner Forge · responsible bo · Aurelius');
      expect(headLine(t, me: 'u-2'), 'owner Forge · responsible You · Aurelius');
    });

    test('"No owner" and no responsible part when there are none', () {
      expect(headLine(ticketOf(projectName: 'Aurelius'), me: 'u-1'), 'No owner · Aurelius');
      expect(headLine(ticketOf(ownerEmployeeId: 'forge', projectName: 'Aurelius'), owner: 'Forge', me: 'u-1'), 'owner Forge · Aurelius');
    });
  });

  group('old links (BR6)', () {
    test('tab=activity opens the Activity sheet; discussion, ticket and none land in the session', () {
      expect(sessionLanding('activity'), TicketLanding.activity);
      expect(sessionLanding('discussion'), TicketLanding.session);
      expect(sessionLanding('ticket'), TicketLanding.session);
      expect(sessionLanding(null), TicketLanding.session);
      expect(sessionLanding('nonsense'), TicketLanding.session);
    });

    test('a number in the route is a number, an id is not', () {
      expect(routeNumber('70'), 70);
      expect(routeNumber('#70'), 70);
      expect(routeNumber('7e1b4f0a-1c2d-4e5f-8a9b-0c1d2e3f4a5b'), isNull);
      expect(routeNumber('k1'), isNull);
      expect(routeNumber(''), isNull);
    });
  });

  group('the status box', () {
    test('a main task that stopped for input in Doing waits for you (Try again)', () {
      final stopped = taskOf(id: 'root', status: TaskStatus.waiting, waitingReason: 'input');
      expect(mainTaskStopped(ticketOf(column: ColumnKind.doing), stopped), isTrue);
      expect(mainTaskStopped(ticketOf(column: ColumnKind.review), stopped), isFalse);
      expect(mainTaskStopped(ticketOf(column: ColumnKind.doing), taskOf(id: 'root', status: TaskStatus.waiting, waitingReason: 'quota')), isFalse);
      expect(mainTaskStopped(ticketOf(column: ColumnKind.doing), null), isFalse);
    });

    test('the last failed answer is the failure, without "send again"; a later good one clears it', () {
      final failed = messageOf('a1', MessageRole.assistant, 'The runtime crashed. Send the message again to retry.', status: MessageStatus.failed);
      expect(lastFailure([messageOf('u1', MessageRole.user, 'Go'), failed]), 'The runtime crashed.');
      expect(lastFailure([failed, messageOf('a2', MessageRole.assistant, 'Done.')]), isNull);
      expect(lastFailure(const []), isNull);
    });

    test('what Try again sends: the last question, else a plain request', () {
      expect(retryText([messageOf('u1', MessageRole.user, 'First'), messageOf('u2', MessageRole.user, 'Second'), messageOf('a', MessageRole.assistant, 'x')]), 'Second');
      expect(retryText(const []), 'Please try again.');
    });

    test("the test task is the tester's latest task of the ticket", () {
      final old = taskOf(id: 'old', employeeId: 'tess', updatedAtMs: 1);
      final latest = taskOf(id: 'new', employeeId: 'tess', updatedAtMs: 9);
      final other = taskOf(id: 'x', employeeId: 'forge', updatedAtMs: 20);
      expect(testTaskOf([old, other, latest], testerId: 'tess')?.id, 'new');
      expect(testTaskOf([old, latest], testerId: null), isNull);
      expect(testTaskOf([other], testerId: 'tess'), isNull);
    });
  });

  group('the run chips under the input (TS-02.5)', () {
    test('hidden while the main task runs; no Mode for a quick task', () {
      final running = ticketOf(column: ColumnKind.doing, rootTaskStatus: TaskStatus.running);
      expect(runChips(running, quick: false), (mode: false, model: false));
      expect(runChips(ticketOf(column: ColumnKind.doing), quick: false), (mode: true, model: true));
      expect(runChips(ticketOf(column: ColumnKind.doing), quick: true), (mode: false, model: true));
    });

    test('the labels: Default for unset, the friendly model name', () {
      expect(modeChipLabel(const TaskRun()), 'Default');
      expect(modeChipLabel(const TaskRun(mode: AgentMode.plan)), 'Plan');
      expect(modelChipLabel(const TaskRun()), 'Default');
      expect(modelChipLabel(const TaskRun(model: 'claude-opus-5-5')), 'Opus 5.5');
    });

    test('the models to choose: every connection\'s, once each, in order', () {
      expect(modelChoices([['claude-opus-5-5', 'claude-sonnet-5-5'], ['claude-sonnet-5-5', 'gpt-5-mini']]), ['claude-opus-5-5', 'claude-sonnet-5-5', 'gpt-5-mini']);
    });
  });
}
