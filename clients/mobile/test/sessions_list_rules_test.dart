import 'package:aurelius_mobile/data/protocol/protocol.dart';
import 'package:aurelius_mobile/features/sessions/recent_use.dart';
import 'package:aurelius_mobile/features/sessions/session_rules.dart';
import 'package:aurelius_mobile/features/sessions/ticket_list_rules.dart';
import 'package:flutter_test/flutter_test.dart';

import 'session_rules_test.dart' show session;
import 'support/tickets.dart' show ticketOf;

// The rules of the Sessions tab's two lists and of the Chat tab, ported from
// the desktop sidebar (recent-use-logic.ts, ticket-session-logic.ts,
// new-session-logic.ts).

// Wednesday 18 November 2026, 15:00 local.
final now = DateTime(2026, 11, 18, 15);
int at(DateTime d) => d.millisecondsSinceEpoch;

void main() {
  group('recent use', () {
    test('lastUsed is the later of the activity and the open time here', () {
      expect(lastUsed({'session:a': 50}, 'session:a', 10), 50);
      expect(lastUsed({'session:a': 5}, 'session:a', 10), 10);
      expect(lastUsed({}, 'session:a', 10), 10);
    });

    test('touchUse keeps only the newest open times', () {
      final uses = touchUse({'a': 1, 'b': 2, 'c': 3}, 'd', 4, max: 3);
      expect(uses.keys, ['d', 'c', 'b']);
    });

    test('parseUses reads nothing from a broken or odd value', () {
      expect(parseUses(null), isEmpty);
      expect(parseUses('not json'), isEmpty);
      expect(parseUses('[1,2]'), isEmpty);
      expect(parseUses('{"a": 3, "b": "x"}'), {'a': 3});
    });
  });

  group('sessionSections', () {
    test('a session opened today here moves to Today, before newer activity', () {
      final old = session(id: 'old', at: at(DateTime(2026, 11, 2)));
      final fresh = session(id: 'fresh', at: at(DateTime(2026, 11, 18, 9)));
      final sections = sessionSections([fresh, old], now, uses: {sessionUseKey('old'): at(DateTime(2026, 11, 18, 14))});
      expect(sections.single.$1, SessionPeriod.today);
      expect(sections.single.$2.map((s) => s.conversationId), ['old', 'fresh']);
    });

    test('without open times the activity alone sorts them, as before', () {
      final sections = sessionSections([session(id: 'a', at: at(DateTime(2026, 11, 18, 8))), session(id: 'b', at: at(DateTime(2026, 11, 18, 9)))], now);
      expect(sections.single.$2.map((s) => s.conversationId), ['b', 'a']);
    });
  });

  group('ticket list', () {
    test('only open columns and no archived tickets are listed', () {
      final tickets = [
        for (final c in ColumnKind.values) ticketOf(id: c.wire, column: c, updatedAtMs: at(DateTime(2026, 11, 18, 10))),
        ticketOf(id: 'archived', column: ColumnKind.doing, archivedAtMs: 1),
      ];
      final listed = ticketSections(tickets, now, needsYou: (_) => false).expand((s) => s.$2).map((t) => t.id).toSet();
      expect(listed, {'todo', 'doing', 'plan_review', 'testing', 'review'});
    });

    test('a ticket that needs you or works is Active, the rest by last use', () {
      final waiting = ticketOf(id: 'mine', column: ColumnKind.review, updatedAtMs: at(DateTime(2026, 10, 1)));
      final running = ticketOf(id: 'runs', column: ColumnKind.doing, rootTaskStatus: TaskStatus.running, updatedAtMs: at(DateTime(2026, 10, 2)));
      final quiet = ticketOf(id: 'quiet', column: ColumnKind.todo, updatedAtMs: at(DateTime(2026, 11, 17, 12)));
      final sections = ticketSections([waiting, running, quiet], now, needsYou: (t) => t.id == 'mine');
      expect([for (final (p, _) in sections) p], [SessionPeriod.active, SessionPeriod.yesterday]);
      expect([for (final (_, list) in sections) [for (final t in list) t.id]], [
        ['runs', 'mine'],
        ['quiet'],
      ]);
    });

    test('ticketSignal: needs you first, then a queued or running main task', () {
      expect(ticketSignal(ticketOf(rootTaskStatus: TaskStatus.running), needsYou: true), SessionSignal.needsYou);
      expect(ticketSignal(ticketOf(rootTaskStatus: TaskStatus.queued), needsYou: false), SessionSignal.working);
      expect(ticketSignal(ticketOf(rootTaskStatus: TaskStatus.waiting), needsYou: false), isNull);
    });
  });

  group('Chat tab', () {
    test('projectRows puts recent projects first and keeps the chosen one', () {
      final shown = [for (var i = 0; i < 8; i++) 'p$i'];
      final r = projectRows(shown, ['p5', 'p2'], 'p7', id: (p) => p);
      expect(r.rows, ['p5', 'p2', 'p0', 'p1', 'p3', 'p7']);
      expect(r.more, isTrue);
      expect(projectRows(['a', 'b'], const [], null, id: (p) => p).more, isFalse);
    });

    test('noteRecent moves a project to the front once', () {
      expect(noteRecent(['a', 'b', 'c'], 'b'), ['b', 'a', 'c']);
    });

    test('continueTickets: Doing, Testing and Review, last updated first, at most three', () {
      final tickets = [
        ticketOf(id: 'todo', column: ColumnKind.todo, updatedAtMs: 9),
        ticketOf(id: 'd1', column: ColumnKind.doing, updatedAtMs: 1),
        ticketOf(id: 't', column: ColumnKind.testing, updatedAtMs: 4),
        ticketOf(id: 'r', column: ColumnKind.review, updatedAtMs: 3),
        ticketOf(id: 'd2', column: ColumnKind.doing, updatedAtMs: 2),
      ];
      expect(continueTickets(tickets).map((t) => t.id), ['t', 'r', 'd2']);
    });
  });
}
