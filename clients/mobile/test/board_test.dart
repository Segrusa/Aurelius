import 'dart:async';

import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/board_bloc.dart';
import 'package:aurelius_mobile/features/board/board_page.dart';
import 'package:flutter/gestures.dart' show kLongPressTimeout;
import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  Widget boardPage(MockRepository repo) => BlocProvider(create: (_) => BoardBloc(repo), child: const BoardPage());

  group('BoardBloc', () {
    test('reads the board for the chosen project (B4) and again on every change (MU1-05 AC3)', () async {
      final changes = StreamController<Event>.broadcast();
      final repo = boardRepository(board: boardOf([ticketOf()]));
      when(() => repo.boardChanges).thenAnswer((_) => changes.stream);
      final bloc = BoardBloc(repo);
      addTearDown(bloc.close);
      await stateWhere(bloc, (s) => !s.loading);
      verify(() => repo.board(projectId: null)).called(1);

      bloc.add(const BoardProjectChosen('p1'));
      await stateWhere(bloc, (s) => s.projectId == 'p1' && !s.loading);
      await untilCalled(() => repo.board(projectId: 'p1'));

      when(() => repo.board(projectId: 'p1')).thenAnswer((_) async => boardOf([ticketOf(column: ColumnKind.todo, version: 4)], projectId: 'p1'));
      changes.add(const Event(aggregateId: 'k1', atMs: 0, kind: EventKind.ticketMoved, seq: 12));
      final moved = await stateWhere(bloc, (s) => s.ticket('k1')?.column == ColumnKind.todo);
      expect(moved.columnOf(ColumnKind.todo)!.tickets.single.version, 4);
      await changes.close();
    });

    test('hidden behind another tab: changes only mark it stale; it reads once when it shows again', () async {
      final changes = StreamController<Event>.broadcast();
      addTearDown(changes.close);
      final repo = boardRepository(board: boardOf([ticketOf()]));
      when(() => repo.boardChanges).thenAnswer((_) => changes.stream);
      final bloc = BoardBloc(repo);
      addTearDown(bloc.close);
      await stateWhere(bloc, (s) => !s.loading);
      verify(() => repo.board(projectId: null)).called(1);

      bloc.add(const BoardShown(false));
      await pumpEventQueue();
      for (var i = 0; i < 3; i++) {
        changes.add(Event(aggregateId: 'k1', atMs: 0, kind: EventKind.ticketMoved, seq: 12 + i));
      }
      await Future<void>.delayed(const Duration(milliseconds: 300));
      verifyNever(() => repo.board(projectId: any(named: 'projectId')));

      bloc.add(const BoardShown(true));
      await Future<void>.delayed(const Duration(milliseconds: 300));
      verify(() => repo.board(projectId: null)).called(1);

      bloc.add(const BoardShown(false));
      bloc.add(const BoardShown(true));
      await Future<void>.delayed(const Duration(milliseconds: 300));
      verifyNever(() => repo.board(projectId: any(named: 'projectId')));
    });

    testWidgets('the watcher tells the bloc when the shell hides the tab (tickers off) and shows it again', (tester) async {
      final changes = StreamController<Event>.broadcast();
      final repo = boardRepository(board: boardOf([ticketOf()]));
      when(() => repo.boardChanges).thenAnswer((_) => changes.stream);
      Widget tab(bool shown) => TickerMode(enabled: shown, child: BlocProvider(create: (_) => BoardBloc(repo), child: const BoardShownWatcher(child: SizedBox())));
      await tester.pumpWidget(tab(true));
      await settleBoard(tester);
      verify(() => repo.board(projectId: null)).called(1);

      await tester.pumpWidget(tab(false));
      changes.add(const Event(aggregateId: 'k1', atMs: 0, kind: EventKind.ticketMoved, seq: 12));
      await settleBoard(tester);
      verifyNever(() => repo.board(projectId: any(named: 'projectId')));

      await tester.pumpWidget(tab(true));
      await settleBoard(tester);
      verify(() => repo.board(projectId: null)).called(1);
      await tester.pumpWidget(const SizedBox());
      await settleBoard(tester);
    });

    test('task events only for tasks under a ticket: its main task or a known delegation', () async {
      final changes = StreamController<Event>.broadcast();
      addTearDown(changes.close);
      final repo = boardRepository(board: boardOf([ticketOf(column: ColumnKind.doing, rootTaskId: 'root', ownerEmployeeId: 'forge')]));
      when(() => repo.boardChanges).thenAnswer((_) => changes.stream);
      final bloc = BoardBloc(repo, ticketTask: (id) => id == 'delegation');
      addTearDown(bloc.close);
      await stateWhere(bloc, (s) => !s.loading);
      verify(() => repo.board(projectId: null)).called(1);

      changes.add(const Event(aggregateId: 'loose', atMs: 0, kind: EventKind.taskQueued, seq: 20));
      await Future<void>.delayed(const Duration(milliseconds: 300));
      verifyNever(() => repo.board(projectId: any(named: 'projectId')));

      changes.add(const Event(aggregateId: 'root', atMs: 0, kind: EventKind.taskQueued, seq: 21));
      await Future<void>.delayed(const Duration(milliseconds: 300));
      verify(() => repo.board(projectId: null)).called(1);

      changes.add(const Event(aggregateId: 'delegation', atMs: 0, kind: EventKind.taskQueued, seq: 22));
      await Future<void>.delayed(const Duration(milliseconds: 300));
      verify(() => repo.board(projectId: null)).called(1);
    });

    test('an older server without the Board: unsupported, not just an error', () async {
      final repo = boardRepository();
      when(() => repo.board(projectId: any(named: 'projectId'))).thenThrow(const ProtocolException(ErrorCode.unknownMethod, 'unknown method board.get'));
      final bloc = BoardBloc(repo);
      addTearDown(bloc.close);
      final s = await stateWhere(bloc, (s) => !s.loading);
      expect(s.unsupported, isTrue);
    });

    test("a Todo card without an owner: the dispatcher's give-up is read from its activity, once per version", () async {
      final repo = boardRepository(board: boardOf([ticketOf(column: ColumnKind.todo)]), activity: [cardOf('ticket.updated', actor: 'system', seq: 3), cardOf('ticket.started', seq: 2)]);
      final bloc = BoardBloc(repo);
      addTearDown(bloc.close);
      final s = await stateWhere(bloc, (s) => !s.loading);
      expect(s.gaveUp, {'k1'});
      bloc.add(const BoardRefreshed());
      await Future<void>.delayed(const Duration(milliseconds: 300));
      verify(() => repo.timeline(correlationId: 'k1', activity: true, limit: 20)).called(1);
    });

    test('a failed read of the activity is not swallowed: the card is unknown, and the board asks again (M-m3)', () async {
      final repo = boardRepository(board: boardOf([ticketOf(column: ColumnKind.todo)]));
      var calls = 0;
      when(() => repo.timeline(correlationId: any(named: 'correlationId'), activity: any(named: 'activity'), limit: any(named: 'limit'))).thenAnswer((_) async {
        if (calls++ == 0) throw const DisconnectedException('The connection dropped.');
        return TimelinePage(cards: [cardOf('ticket.updated', actor: 'system', seq: 3), cardOf('ticket.started', seq: 2)]);
      });
      final bloc = BoardBloc(repo, retryAfter: const Duration(milliseconds: 50));
      addTearDown(bloc.close);
      final first = await stateWhere(bloc, (s) => !s.loading);
      expect(first.ownerUnknown, {'k1'});
      expect(first.gaveUp, isEmpty);
      final again = await stateWhere(bloc, (s) => s.ownerUnknown.isEmpty);
      expect(again.gaveUp, {'k1'}, reason: 'read again without a new event');
    });

    test('the owner filter keeps the columns and filters the cards', () async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', ownerEmployeeId: 'forge'), ticketOf(id: 'b')]));
      final bloc = BoardBloc(repo);
      addTearDown(bloc.close);
      await stateWhere(bloc, (s) => !s.loading);
      bloc.add(const BoardOwnerChosen(BoardState.noOwner));
      final s = await stateWhere(bloc, (s) => s.owner == BoardState.noOwner);
      expect(s.columns, hasLength(6));
      expect(s.tickets.map((t) => t.id), ['b']);
    });
  });

  group('the Board on a phone (§4.1)', () {
    testWidgets('six columns in order as chips with counts, one column at a time (MU1-05 AC1)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', number: 5, title: 'Vault UI', priority: TicketPriority.high, labels: const ['ui', 'vault', 'security'])]));
      await pumpBoardPage(tester, repo, boardPage(repo));
      final chips = [for (final (kind, _) in defaultColumns) tester.getTopLeft(find.byKey(ValueKey('board-chip-${kind.wire}'))).dx];
      expect(chips, orderedEquals([...chips]..sort()), reason: 'Backlog · Todo · Doing · Plan review · Review · Done');
      expect(find.text('Backlog 1'), findsOneWidget);
      expect(find.text('Plan review 0'), findsOneWidget);
      // Nothing needs you: it opens on Doing.
      expect(find.byKey(const ValueKey('board-empty-doing')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('board-chip-backlog')));
      await settleBoard(tester);
      expect(find.text('Vault UI'), findsOneWidget);
      expect(find.text('▲ High'), findsOneWidget, reason: 'shape and word');
      expect(find.text('+1'), findsOneWidget, reason: 'two labels and a count');
      expect(find.text('by ada'), findsOneWidget, reason: 'the creator');
      expect(find.text('▸ Start'), findsOneWidget, reason: 'Start is always visible in Backlog');
    });

    testWidgets('it opens on what needs you, and counts it: "2 tickets · 1 needs you"', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', title: 'Draft'), ticketOf(id: 'b', number: 40, title: 'Argon2 hashing', column: ColumnKind.review, ownerName: 'Forge', ownerEmployeeId: 'forge')]));
      await pumpBoardPage(tester, repo, boardPage(repo));
      expect(find.text('2 tickets · 1 needs you'), findsOneWidget);
      expect(find.text('Argon2 hashing'), findsOneWidget, reason: 'Review is shown first');
      expect(find.text('Report to review'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('card-action-b')));
      await settleBoard(tester);
      expect(find.text('ticket b'), findsOneWidget, reason: 'Review opens the ticket at its review gate');
    });

    testWidgets('Start: Backlog → Todo with the version, the toast says who chooses, Undo cancels (MU1-09, §6)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', number: 51)]));
      await pumpBoardPage(tester, repo, boardPage(repo));
      await tester.tap(find.byKey(const ValueKey('board-chip-backlog')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('card-action-a')));
      await settleBoard(tester);
      verify(() => repo.write(any(that: containsPair('method', 'ticket.start')), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: 3, retry: true)).called(1);
      expect(find.text('#51 started. Orion chooses an employee.'), findsOneWidget);
      when(() => repo.ticket('a')).thenAnswer((_) async => TicketDetail(asOfSeq: 2, ticket: ticketOf(id: 'a', number: 51, column: ColumnKind.todo, version: 4), tasks: const []));
      await tester.tap(find.text('Undo'));
      await settleBoard(tester);
      final cancel = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: 4, retry: false)).captured.single as Map<String, dynamic>;
      expect(cancel['method'], 'ticket.cancel', reason: 'with the version it was read at');
      expect(paramsOf(cancel), {'ticket_id': 'a'});
    });

    testWidgets('the Start toast with Undo goes by itself after 5 s (§6)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', number: 51)]));
      await pumpBoardPage(tester, repo, boardPage(repo));
      await tester.tap(find.byKey(const ValueKey('board-chip-backlog')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('card-action-a')));
      await settleBoard(tester);
      expect(find.text('Undo'), findsOneWidget);
      expect(tester.widget<SnackBar>(find.byType(SnackBar)).persist, isFalse);
      await tester.pump(const Duration(seconds: 6));
      await tester.pumpAndSettle();
      expect(find.text('#51 started. Orion chooses an employee.'), findsNothing);
      expect(find.text('Undo'), findsNothing);
    });

    testWidgets('Undo after the dispatcher already chose cancels nothing and says so (§6)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', number: 51)]));
      await pumpBoardPage(tester, repo, boardPage(repo));
      await tester.tap(find.byKey(const ValueKey('board-chip-backlog')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('card-action-a')));
      await settleBoard(tester);
      when(() => repo.ticket('a')).thenAnswer((_) async => TicketDetail(asOfSeq: 3, ticket: ticketOf(id: 'a', number: 51, column: ColumnKind.todo, ownerEmployeeId: 'forge', ownerName: 'Forge', version: 5), tasks: const []));
      await tester.tap(find.text('Undo'));
      await settleBoard(tester);
      expect(writesOf(repo).map((w) => w['method']), ['ticket.start'], reason: 'no ticket.cancel once an owner is chosen');
      expect(find.text('#51 already went to Forge; use Back to Backlog to stop it.'), findsOneWidget);
    });

    testWidgets('swipe left opens Move to…: forbidden columns stay, dimmed, with the reason; Backlog asks first (§4.2)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', number: 42, column: ColumnKind.doing, ownerName: 'Forge', ownerEmployeeId: 'forge', rootTaskStatus: TaskStatus.running)]));
      await pumpBoardPage(tester, repo, boardPage(repo));
      await tester.drag(find.byKey(const ValueKey('swipe-a')), const Offset(-500, 0));
      await settleBoard(tester);
      expect(find.text('Move #42 Write the release notes'), findsOneWidget);
      expect(find.text('Not from Doing.'), findsOneWidget);
      expect(find.text('The engine moves tickets here.'), findsOneWidget);
      expect(find.text('Only from Review.'), findsOneWidget);
      expect(tester.widget<ListTile>(find.byKey(const ValueKey('move-to-done'))).enabled, isFalse);
      await tester.tap(find.byKey(const ValueKey('move-to-backlog')));
      await settleBoard(tester);
      expect(find.text('This stops the main task and its delegations.'), findsOneWidget);
      await tester.tap(find.widgetWithText(TextButton, 'Back to Backlog'));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.cancel');
    });

    testWidgets('long press: Move down sends ticket.move with before_ticket_id (no dragging on a phone)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', title: 'First'), ticketOf(id: 'b', title: 'Second'), ticketOf(id: 'c', title: 'Third')]));
      await pumpBoardPage(tester, repo, boardPage(repo));
      await tester.tap(find.byKey(const ValueKey('board-chip-backlog')));
      await settleBoard(tester);
      await tester.longPress(find.text('First'));
      await settleBoard(tester);
      expect(find.text('Move up'), findsNothing, reason: 'already on top');
      await tester.tap(find.text('Move down'));
      await settleBoard(tester);
      final move = writesOf(repo).single;
      expect(move['method'], 'ticket.move');
      expect(paramsOf(move), {'ticket_id': 'a', 'column_id': 'col-backlog', 'before_ticket_id': 'c'});
    });

    testWidgets('a Todo ticket nobody dispatches says "Choose an employee" and lets you choose (B5, MU1-12 AC2)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', number: 47, column: ColumnKind.todo)]));
      await pumpBoardPage(tester, repo, boardPage(repo), company: companyWith(dispatcher: null));
      expect(find.text('Choose an employee'), findsOneWidget);
      expect(find.text('1 ticket · 1 needs you'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('card-action-a')));
      await settleBoard(tester);
      expect(find.byKey(const ValueKey('owner-dispatcher')), findsNothing, reason: 'in Todo only an employee');
      await tester.tap(find.byKey(const ValueKey('owner-forge')));
      await settleBoard(tester);
      final assign = writesOf(repo).single;
      expect(assign['method'], 'ticket.assign');
      expect(paramsOf(assign), {'ticket_id': 'a', 'employee_id': 'forge'});
    });

    testWidgets('when its activity cannot be read, a Todo card needs you instead of "choosing…" (M-m3)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', column: ColumnKind.todo)]));
      when(() => repo.timeline(correlationId: any(named: 'correlationId'), activity: any(named: 'activity'), limit: any(named: 'limit')))
          .thenThrow(const ProtocolException(ErrorCode.internal, 'timeline failed'));
      await pumpBoardPage(tester, repo, boardPage(repo));
      expect(find.text('1 ticket · 1 needs you'), findsOneWidget);
      expect(find.text('Dispatcher unknown · choose an employee'), findsOneWidget);
      expect(find.text('Orion is choosing…'), findsNothing);
      expect(find.byKey(const ValueKey('card-action-a')), findsOneWidget, reason: 'Choose owner');
    });

    testWidgets('while the dispatcher chooses, the card says so', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', column: ColumnKind.todo)]));
      await pumpBoardPage(tester, repo, boardPage(repo));
      await tester.tap(find.byKey(const ValueKey('board-chip-todo')));
      await settleBoard(tester);
      expect(find.text('Orion is choosing…'), findsOneWidget);
      expect(find.byKey(const ValueKey('card-action-a')), findsNothing);
    });

    testWidgets('the project filter reads the board of that project; an empty filter says so with Clear filter', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf()]));
      when(() => repo.board(projectId: 'p1')).thenAnswer((_) async => boardOf(const [], projectId: 'p1'));
      await pumpBoardPage(tester, repo, boardPage(repo));
      await tester.tap(find.byKey(const ValueKey('board-filter-project')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('board-project-p1')));
      await settleBoard(tester);
      verify(() => repo.board(projectId: 'p1')).called(greaterThanOrEqualTo(1));
      expect(find.text('No tickets in Atlas.'), findsOneWidget);
      await tester.tap(find.text('Clear filter'));
      await settleBoard(tester);
      expect(find.text('No tickets in Atlas.'), findsNothing);
    });

    testWidgets('an empty board after the clean start: "No tickets yet" and New ticket (MU1-19 AC4)', (tester) async {
      final repo = boardRepository();
      await pumpBoardPage(tester, repo, boardPage(repo));
      await tester.tap(find.byKey(const ValueKey('board-chip-backlog')));
      await settleBoard(tester);
      expect(find.text('No tickets yet'), findsOneWidget);
      expect(find.text('New tickets land in Backlog. Nothing starts until you press Start.'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('board-empty-new')));
      await settleBoard(tester);
      expect(find.text('new ticket '), findsOneWidget);
    });

    testWidgets('a board that cannot load: the reason and Try again; an older server: Update', (tester) async {
      final repo = boardRepository();
      when(() => repo.board(projectId: any(named: 'projectId'))).thenThrow(const ProtocolException(ErrorCode.internal, 'The database is busy.'));
      await pumpBoardPage(tester, repo, boardPage(repo));
      expect(find.text('Could not load the Board.'), findsOneWidget);
      expect(find.text('The database is busy.'), findsOneWidget);
      when(() => repo.board(projectId: any(named: 'projectId'))).thenAnswer((_) async => boardOf([ticketOf(title: 'Back again')]));
      await tester.tap(find.byKey(const ValueKey('board-retry')));
      await settleBoard(tester);
      expect(find.text('Could not load the Board.'), findsNothing);

      final old = boardRepository();
      when(() => old.board(projectId: any(named: 'projectId'))).thenThrow(const ProtocolException(ErrorCode.unknownMethod, 'unknown'));
      await pumpBoardPage(tester, old, boardPage(old));
      expect(find.text('This server does not have the Board yet.'), findsOneWidget);
    });

    testWidgets('without projects: create one first', (tester) async {
      final repo = boardRepository();
      await pumpBoardPage(tester, repo, boardPage(repo), company: companyWith(projects: const []));
      expect(find.text('Create a project first; work is done in a project.'), findsOneWidget);
    });

    testWidgets('offline the actions are off and the board says it reconnects (§3.3)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a')]));
      await pumpBoardPage(tester, repo, boardPage(repo), link: offlineLink);
      await tester.tap(find.byKey(const ValueKey('board-chip-backlog')));
      await settleBoard(tester);
      expect(find.textContaining('Offline · reconnecting'), findsOneWidget);
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('card-action-a'))).onPressed, isNull);
    });

    testWidgets('screen readers: a summary per card and its actions (§15)', (tester) async {
      final handle = tester.ensureSemantics();
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', number: 51, title: 'Vault UI', priority: TicketPriority.urgent)]));
      await pumpBoardPage(tester, repo, boardPage(repo));
      await tester.tap(find.byKey(const ValueKey('board-chip-backlog')));
      await settleBoard(tester);
      final node = tester.getSemantics(find.bySemanticsLabel(RegExp(r'^Ticket 51, Vault UI, priority Urgent')));
      final actions = node.getSemanticsData().customSemanticsActionIds!.map((id) => CustomSemanticsAction.getAction(id)!.label);
      expect(actions, containsAll(['Open', 'Start', 'Move to…']));
      expect(find.bySemanticsLabel(RegExp(r'^Backlog, tab 1 of 6, 1 ticket')), findsOneWidget);
      handle.dispose();
    });
  });

  group('the Board on a tablet (§4.3)', () {
    const tablet = Size(1180, 820);

    testWidgets('from 700 px the columns stand side by side; an empty Plan review is a strip (D-04)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', title: 'In backlog'), ticketOf(id: 'b', title: 'In review', column: ColumnKind.review)]));
      await pumpBoardPage(tester, repo, boardPage(repo), size: tablet);
      expect(find.byKey(const ValueKey('board-wide')), findsOneWidget);
      expect(find.byKey(const ValueKey('board-chips')), findsNothing);
      expect(find.text('In backlog'), findsOneWidget);
      expect(find.text('In review'), findsOneWidget);
      expect(find.byKey(const ValueKey('board-plan-review-strip')), findsOneWidget);
      expect(find.text('◆ Needs you'), findsOneWidget, reason: 'Review has something to review');
    });

    Future<void> drag(WidgetTester tester, Finder from, Finder to) async {
      final gesture = await tester.startGesture(tester.getCenter(from));
      await tester.pump(kLongPressTimeout + const Duration(milliseconds: 100));
      await gesture.moveTo(tester.getCenter(to));
      await tester.pump(const Duration(milliseconds: 100));
      await gesture.moveTo(tester.getCenter(to) + const Offset(0, 1));
      await tester.pump(const Duration(milliseconds: 100));
      await gesture.up();
      await settleBoard(tester);
    }

    testWidgets('drag a Backlog card onto Todo: Start, as the button (MU1-09 AC2)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', title: 'Drag me')]));
      await pumpBoardPage(tester, repo, boardPage(repo), size: tablet);
      await drag(tester, find.text('Drag me'), find.byKey(const ValueKey('board-empty-todo')));
      expect(writesOf(repo).single['method'], 'ticket.start');
    });

    testWidgets('drag a card onto another in its column: it lands above it (ticket.move before_ticket_id)', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', title: 'Top card'), ticketOf(id: 'b', title: 'Middle card'), ticketOf(id: 'c', title: 'Bottom card')]));
      await pumpBoardPage(tester, repo, boardPage(repo), size: tablet);
      await drag(tester, find.text('Bottom card'), find.text('Top card'));
      final move = writesOf(repo).single;
      expect(move['method'], 'ticket.move');
      expect(paramsOf(move), {'ticket_id': 'c', 'column_id': 'col-backlog', 'before_ticket_id': 'a'});
    });

    testWidgets('a drop the rules forbid sends nothing', (tester) async {
      final repo = boardRepository(board: boardOf([ticketOf(id: 'a', title: 'Not yet')]));
      await pumpBoardPage(tester, repo, boardPage(repo), size: tablet);
      await drag(tester, find.text('Not yet'), find.byKey(const ValueKey('board-empty-doing')));
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
    });
  });
}
