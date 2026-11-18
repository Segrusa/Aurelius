import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/tickets.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/board_bloc.dart';
import 'package:aurelius_mobile/features/board/board_page.dart';
import 'package:aurelius_mobile/features/board/board_settings_page.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// Ticket #5: the Testing column between Plan review and Review, where the
// company's tester runs the full test suite once. Shown only when the server
// has `board_testing`; a hand only takes a card out of it to Backlog.

final pulse = employeeOf(id: 'pulse', name: 'Pulse', role: 'QA Engineer');

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  final tested = ticketOf(id: 't', number: 7, title: 'Argon2 hashing', column: ColumnKind.testing, ownerEmployeeId: 'forge', ownerName: 'Forge', rootTaskId: 'root', rootTaskStatus: TaskStatus.review);
  CompanyState withTester() => companyWith(tester: 'pulse', employees: [orion, forge, pulse]);

  group('the rules (domain::ticket::decide_move)', () {
    test('out of Testing only to Backlog, with a confirmation; nothing moves into it', () {
      expect(moveRule(ColumnKind.testing, ColumnKind.backlog), isA<MoveAllowed>().having((r) => r.kind, 'kind', MoveKind.cancel).having((r) => r.confirm, 'confirm', isNotNull));
      expect(moveRule(ColumnKind.testing, ColumnKind.testing), isA<MoveAllowed>().having((r) => r.kind, 'kind', MoveKind.reorder));
      for (final to in [ColumnKind.todo, ColumnKind.doing, ColumnKind.planReview, ColumnKind.review]) {
        expect(moveRule(ColumnKind.testing, to), isA<MoveRefused>().having((r) => r.reason, 'reason', testingMovesByItself), reason: 'Testing → ${to.wire}');
      }
      expect(moveRule(ColumnKind.testing, ColumnKind.done), isA<MoveRefused>(), reason: 'only Done in Review closes a ticket');
      for (final from in [ColumnKind.backlog, ColumnKind.todo, ColumnKind.doing, ColumnKind.planReview, ColumnKind.review]) {
        expect(moveRule(from, ColumnKind.testing), isA<MoveRefused>().having((r) => r.reason, 'reason', testingMovesByItself), reason: '${from.wire} → Testing');
      }
      expect(moveRule(ColumnKind.done, ColumnKind.testing), isA<MoveRefused>());
    });

    test('a card in Testing is being tested by the tester, and needs nobody', () {
      final state = ticketState(tested);
      expect(state, TicketState.testing);
      expect(state.needsYou, isFalse);
      expect(state.line(tester: 'Pulse'), 'Pulse is testing…');
      expect(state.line(), 'The tester is testing…');
      expect(ColumnKind.testing.role, 'Test gate');
      expect(ColumnKind.testing.emptyText, isNotEmpty);
    });

    test('a word in Testing goes to the owner, as in Review', () {
      expect(discussionAddressee(tested, owner: 'Forge'), 'Forge');
      expect(discussionHint(tested, owner: 'Forge'), 'Message Forge (owner) · @ to mention someone else');
    });

    test('ticket.tested reads "Tests passed" or "Tests failed"', () {
      TimelineCard testedCard(String detail) => TimelineCard(actor: 'system', atMs: 0, correlationId: 't', detail: detail, kind: 'activity', label: 'ACTIVITY', seq: 1, title: 'ticket.tested');
      expect(activityText(testedCard('passed')), 'Tests passed');
      expect(activityText(testedCard('failed')), 'Tests failed');
      expect(activityText(testedCard('ticket')), 'The tests ran', reason: 'a server that does not say which');
    });

    test('without board_testing the Testing column is left out', () {
      final board = boardOf(const [], testing: true);
      expect(boardShown(board, testing: true).columns.map((c) => c.column.kind), contains(ColumnKind.testing));
      expect(boardShown(board, testing: false).columns.map((c) => c.column.kind), [for (final (k, _) in defaultColumns) k]);
    });
  });

  group('BoardBloc', () {
    blocTest<BoardBloc, BoardState>(
      'shows Testing only when the server has board_testing',
      build: () => BoardBloc(boardRepository(board: boardOf([tested], testing: true), link: testingLink)),
      wait: const Duration(milliseconds: 400),
      verify: (bloc) {
        expect(bloc.state.columns.map((c) => c.column.kind), [for (final (k, _) in testingColumns) k]);
        expect(bloc.state.columnOf(ColumnKind.testing)!.tickets.single.id, 't');
      },
    );

    blocTest<BoardBloc, BoardState>(
      'leaves Testing out on a server without board_testing',
      build: () => BoardBloc(boardRepository(board: boardOf(const [], testing: true))),
      wait: const Duration(milliseconds: 400),
      verify: (bloc) => expect(bloc.state.columnOf(ColumnKind.testing), isNull),
    );
  });

  group('the Board', () {
    Widget boardPage(MockRepository repo, {bool inProject = false}) => BlocProvider(create: (_) => BoardBloc(repo, projectId: inProject ? 'p1' : null), child: BoardPage(inProject: inProject));

    testWidgets('a phone: the Testing chip between Plan review and Review; the card says Pulse is testing', (tester) async {
      final repo = boardRepository(board: boardOf([tested], testing: true), link: testingLink);
      await pumpBoardPage(tester, repo, boardPage(repo), company: withTester(), link: testingLink);
      final x = [for (final kind in [ColumnKind.planReview, ColumnKind.testing, ColumnKind.review]) tester.getTopLeft(find.byKey(ValueKey('board-chip-${kind.wire}'))).dx];
      expect(x, orderedEquals([...x]..sort()));
      expect(find.text('Testing 1'), findsOneWidget);
      await tester.ensureVisible(find.byKey(const ValueKey('board-chip-testing')));
      await tester.tap(find.byKey(const ValueKey('board-chip-testing')));
      await settleBoard(tester);
      expect(find.text('Pulse is testing…'), findsOneWidget);
      expect(find.byKey(const ValueKey('card-action-t')), findsNothing, reason: 'nothing for you to do');
      expect(find.text('1 ticket'), findsOneWidget, reason: 'it needs nobody');
    });

    testWidgets("the project's Board tab shows Testing too", (tester) async {
      final repo = boardRepository(board: boardOf([tested], projectId: 'p1', testing: true), link: testingLink);
      await pumpBoardPage(tester, repo, Scaffold(body: boardPage(repo, inProject: true)), company: withTester(), link: testingLink);
      expect(find.byKey(const ValueKey('board-chip-testing')), findsOneWidget);
      verify(() => repo.board(projectId: 'p1')).called(greaterThan(0));
    });

    testWidgets('without board_testing there is no Testing chip', (tester) async {
      final repo = boardRepository(board: boardOf(const [], testing: true));
      await pumpBoardPage(tester, repo, boardPage(repo));
      expect(find.byKey(const ValueKey('board-chip-testing')), findsNothing);
      expect(find.byKey(const ValueKey('board-chip-review')), findsOneWidget);
    });

    testWidgets('Move to… from Testing: only Backlog, after a confirmation, then ticket.cancel', (tester) async {
      final repo = boardRepository(board: boardOf([tested], testing: true), link: testingLink);
      await pumpBoardPage(tester, repo, boardPage(repo), company: withTester(), link: testingLink);
      await tester.ensureVisible(find.byKey(const ValueKey('board-chip-testing')));
      await tester.tap(find.byKey(const ValueKey('board-chip-testing')));
      await settleBoard(tester);
      await tester.drag(find.byKey(const ValueKey('swipe-t')), const Offset(-500, 0));
      await settleBoard(tester);
      for (final kind in [ColumnKind.todo, ColumnKind.doing, ColumnKind.planReview, ColumnKind.review, ColumnKind.done]) {
        expect(tester.widget<ListTile>(find.byKey(ValueKey('move-to-${kind.wire}'))).enabled, isFalse, reason: kind.wire);
      }
      expect(find.text(testingMovesByItself), findsNWidgets(4));
      await tester.tap(find.byKey(const ValueKey('move-to-backlog')));
      await settleBoard(tester);
      expect(find.text('This stops the tests and the work on the ticket.'), findsOneWidget);
      await tester.tap(find.widgetWithText(TextButton, 'Back to Backlog'));
      await settleBoard(tester);
      final cancel = writesOf(repo).single;
      expect(cancel['method'], 'ticket.cancel');
      expect(paramsOf(cancel), {'ticket_id': 't'});
    });

    testWidgets('a tablet: a card dropped on Testing, or from Testing on Review, sends nothing', (tester) async {
      final doing = ticketOf(id: 'd', title: 'Busy card', column: ColumnKind.doing, ownerEmployeeId: 'forge', rootTaskStatus: TaskStatus.running);
      final repo = boardRepository(board: boardOf([doing, tested], testing: true), link: testingLink);
      await pumpBoardPage(tester, repo, boardPage(repo), company: withTester(), link: testingLink, size: const Size(1600, 820));
      Future<void> drag(Finder from, Finder to) async {
        final gesture = await tester.startGesture(tester.getCenter(from));
        await tester.pump(kLongPressTimeout + const Duration(milliseconds: 100));
        await gesture.moveTo(tester.getCenter(to));
        await tester.pump(const Duration(milliseconds: 100));
        await gesture.moveTo(tester.getCenter(to) + const Offset(0, 1));
        await tester.pump(const Duration(milliseconds: 100));
        await gesture.up();
        await settleBoard(tester);
      }

      await drag(find.text('Busy card'), find.text('Argon2 hashing'));
      await drag(find.text('Argon2 hashing'), find.byKey(const ValueKey('board-empty-review')));
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
    });
  });

  group('the ticket page in Testing', () {
    testWidgets('no Done or Send back; it says who tests and opens the test task; Back to Backlog is in More', (tester) async {
      final root = taskOf(id: 'root', status: TaskStatus.review, result: 'Hashing done.');
      final test = taskOf(id: 'test-1', status: TaskStatus.running, employeeId: 'pulse', employeeName: 'Pulse', parentTaskId: 'root', updatedAtMs: 5);
      final repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: tested, rootTask: root, tasks: [root, test]), link: testingLink);
      final router = await pumpBoardPage(tester, repo, TicketPage(ticketId: tested.id), company: withTester(), link: testingLink);
      expect(find.text('Testing'), findsOneWidget, reason: 'the column');
      expect(find.text('Pulse is testing…'), findsOneWidget);
      expect(find.byKey(const ValueKey('ticket-testing')), findsOneWidget);
      expect(find.byKey(const ValueKey('bar-done')), findsNothing);
      expect(find.byKey(const ValueKey('bar-send-back')), findsNothing);
      await tester.tap(find.byKey(const ValueKey('bar-open-test-task')));
      await settleBoard(tester);
      expect(find.text('task test-1'), findsOneWidget);
      router.pop();
      await settleBoard(tester);
      await tester.tap(find.byTooltip('Ticket actions'));
      await settleBoard(tester);
      await tester.tap(find.text('Back to Backlog').last);
      await settleBoard(tester);
      expect(find.text('This stops the tests and the work on the ticket.'), findsOneWidget);
      await tester.tap(find.widgetWithText(TextButton, 'Back to Backlog'));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.cancel');
    });
  });

  group('the tester in Settings → Board', () {
    testWidgets('with board_testing: None or an employee, sent as company.update with the version', (tester) async {
      final repo = boardRepository(board: boardOf(const [], testing: true), link: testingLink);
      final company = MockCompanyBloc();
      await pumpBoardPage(tester, repo, const BoardSettingsPage(), company: withTester(), companyBloc: company, link: testingLink);
      expect(find.text('Test gate · Used only with a tester'), findsOneWidget, reason: 'the seventh column in the editor');
      await tester.scrollUntilVisible(find.byKey(const ValueKey('company-tester')), 200, scrollable: find.byType(Scrollable).first);
      expect(find.descendant(of: find.byKey(const ValueKey('company-tester')), matching: find.text('Pulse')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('company-tester')));
      await settleBoard(tester);
      expect(find.text('None: no Testing step'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('tester-none')));
      await settleBoard(tester);
      final sent = verify(() => company.send(captureAny(), expectedVersion: 4, retry: false, done: any(named: 'done'))).captured.single as Map<String, dynamic>;
      expect(sent['method'], 'company.update');
      expect(paramsOf(sent), {'tester': <String, dynamic>{}});

      await tester.tap(find.byKey(const ValueKey('company-tester')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('tester-forge')));
      await settleBoard(tester);
      final chosen = verify(() => company.send(captureAny(), expectedVersion: 4, retry: false, done: any(named: 'done'))).captured.single as Map<String, dynamic>;
      expect(paramsOf(chosen), {'tester': {'employee_id': 'forge'}});
    });

    testWidgets('without board_testing there is no tester', (tester) async {
      final repo = boardRepository(board: boardOf(const []));
      await pumpBoardPage(tester, repo, const BoardSettingsPage());
      expect(find.byKey(const ValueKey('company-tester')), findsNothing);
      expect(find.byKey(const ValueKey('company-dispatcher')), findsOneWidget);
    });

    testWidgets('a refusal shows the server\'s message', (tester) async {
      final repo = boardRepository(board: boardOf(const [], testing: true), link: testingLink);
      final company = MockCompanyBloc();
      final start = withTester();
      await pumpBoardPage(tester, repo, const BoardSettingsPage(), company: start, companyStates: Stream.fromIterable([start.copyWith(notice: 'A company tester is not supported yet.')]), companyBloc: company, link: testingLink);
      expect(find.text('A company tester is not supported yet.'), findsOneWidget);
    });
  });
}
