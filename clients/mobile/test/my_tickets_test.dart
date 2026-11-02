import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart' show LinkState, LinkStatus;
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/board_bloc.dart';
import 'package:aurelius_mobile/features/board/board_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/tickets.dart';

// A4 (US-A4, ui-ontwerp §5, README): My tickets as a segmented control in the
// Board, four tabs made from one `ticket.my_tickets`; nothing of it on a server without the feature.

const _myTicketsLink = LinkState(status: LinkStatus.online, capabilities: ['read', 'chat', 'task_control', 'approve'], features: ['ticket_responsible', 'ticket_follow', 'message_locate', 'my_tickets'], boardMembersExist: true, user: testUser, claudeLogin: testLogin);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  Widget board(MockRepository repo) => BlocProvider(create: (_) => BoardBloc(repo), child: const BoardPage());

  MockRepository repoWith(List<TicketView> mine, {List<String> following = const [], LinkState link = _myTicketsLink}) {
    final repo = boardRepository(board: boardOf(mine), link: link);
    when(() => repo.myTickets(projectId: any(named: 'projectId'), includeDone: any(named: 'includeDone'))).thenAnswer((_) async => MyTicketsView(tickets: mine, following: following));
    return repo;
  }

  Future<void> openMine(WidgetTester tester, MockRepository repo, {LinkState link = _myTicketsLink}) async {
    await pumpBoardPage(tester, repo, board(repo), link: link);
    await tester.tap(find.text('My tickets'));
    await settleBoard(tester);
  }

  Future<void> chooseTab(WidgetTester tester, String name) async {
    await tester.ensureVisible(find.byKey(ValueKey('my-tab-$name')));
    await tester.tap(find.byKey(ValueKey('my-tab-$name')));
    await settleBoard(tester);
  }

  final mineResponsible = ticketOf(id: 'k1', number: 1, column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge', responsible: testUser, creator: bo, title: 'Responsible one');
  final mineCreated = ticketOf(id: 'k2', number: 2, column: ColumnKind.backlog, responsible: bo, title: 'Created one');
  final followed = ticketOf(id: 'k3', number: 3, column: ColumnKind.todo, creator: bo, responsible: bo, title: 'Followed one');
  final inReview = ticketOf(id: 'k4', number: 4, column: ColumnKind.review, creator: bo, responsible: testUser, title: 'Waiting one');
  final othersReview = ticketOf(id: 'k5', number: 5, column: ColumnKind.review, creator: bo, responsible: bo, title: 'Someone elses review');

  testWidgets('a server without my_tickets has no switch', (tester) async {
    final repo = repoWith([mineResponsible], link: phaseALink);
    await pumpBoardPage(tester, repo, board(repo), link: phaseALink);
    expect(find.byKey(const ValueKey('board-switch')), findsNothing);
    verifyNever(() => repo.myTickets(projectId: any(named: 'projectId'), includeDone: any(named: 'includeDone')));
  });

  testWidgets('the Board stays the first view; My tickets reads nothing until it is chosen', (tester) async {
    final repo = repoWith([mineResponsible]);
    await pumpBoardPage(tester, repo, board(repo), link: _myTicketsLink);
    expect(find.byKey(const ValueKey('board-switch')), findsOneWidget);
    expect(find.byKey(const ValueKey('board-count')), findsOneWidget);
    verifyNever(() => repo.myTickets(projectId: any(named: 'projectId'), includeDone: any(named: 'includeDone')));
  });

  testWidgets('four tabs with counts from one answer; Waiting on me opens first and counts only what is yours (B6)', (tester) async {
    final repo = repoWith([mineResponsible, mineCreated, followed, inReview, othersReview], following: ['k3']);
    await openMine(tester, repo);
    verify(() => repo.myTickets(projectId: null, includeDone: false)).called(1);
    expect(find.text('Responsible 2'), findsOneWidget);
    expect(find.text('Created by me 0'), findsNothing, reason: 'creator of k2 is the test user: counted');
    expect(find.text('Created by me 1'), findsOneWidget);
    expect(find.text('Following 1'), findsOneWidget);
    expect(find.text('Waiting on me 1'), findsOneWidget, reason: "k5's review is bo's");
    expect(find.text('Waiting one'), findsOneWidget);
    expect(find.text('Someone elses review'), findsNothing);
  });

  testWidgets('without anything waiting the first tab is Responsible; a chip chooses another', (tester) async {
    final repo = repoWith([mineResponsible, mineCreated, followed], following: ['k3']);
    await openMine(tester, repo);
    expect(find.text('Responsible one'), findsOneWidget);
    expect(find.text('Created one'), findsNothing);
    await chooseTab(tester, 'created');
    expect(find.text('Created one'), findsOneWidget);
    expect(find.text('Responsible one'), findsNothing);
    await chooseTab(tester, 'following');
    expect(find.text('Followed one'), findsOneWidget);
  });

  testWidgets('Created by me with nothing: the words and New ticket', (tester) async {
    final repo = repoWith([followed], following: ['k3']);
    await openMine(tester, repo);
    await chooseTab(tester, 'created');
    expect(find.text("You haven't created a ticket yet."), findsOneWidget);
    expect(find.byKey(const ValueKey('my-tickets-new')), findsOneWidget);
    await chooseTab(tester, 'waiting');
    expect(find.text('Nothing waits for you.'), findsOneWidget);
    await chooseTab(tester, 'following');
    expect(find.text('Followed one'), findsOneWidget);
  });

  testWidgets('a failed read says what failed with Try again, never an endless skeleton', (tester) async {
    final repo = repoWith([mineResponsible]);
    when(() => repo.myTickets(projectId: any(named: 'projectId'), includeDone: any(named: 'includeDone'))).thenThrow(const ProtocolException(ErrorCode.internal, 'boom'));
    await openMine(tester, repo);
    expect(find.text('Could not load your tickets.'), findsOneWidget);
    expect(find.byKey(const ValueKey('my-tickets-skeleton')), findsNothing);
    when(() => repo.myTickets(projectId: any(named: 'projectId'), includeDone: any(named: 'includeDone'))).thenAnswer((_) async => MyTicketsView(tickets: [mineResponsible], following: const []));
    await tester.tap(find.byKey(const ValueKey('my-tickets-retry')));
    await settleBoard(tester);
    expect(find.text('Responsible one'), findsOneWidget);
  });

  testWidgets('Show Done asks for include_done; a project filter without tickets offers Clear filter', (tester) async {
    final repo = repoWith([mineResponsible]);
    await openMine(tester, repo);
    await tester.tap(find.byKey(const ValueKey('my-show-done')));
    await settleBoard(tester);
    verify(() => repo.myTickets(projectId: null, includeDone: true)).called(1);

    when(() => repo.myTickets(projectId: 'p1', includeDone: any(named: 'includeDone'))).thenAnswer((_) async => const MyTicketsView(tickets: [], following: []));
    await tester.tap(find.byKey(const ValueKey('my-filter-project')));
    await settleBoard(tester);
    await tester.tap(find.byKey(const ValueKey('my-project-p1')));
    await settleBoard(tester);
    expect(find.text('No tickets in Atlas.'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('my-tickets-clear')));
    await settleBoard(tester);
    expect(find.text('Responsible one'), findsOneWidget);
  });

  testWidgets('a change to a ticket reads the lists again while they show', (tester) async {
    final changes = StreamController<Event>.broadcast();
    addTearDown(changes.close);
    final repo = repoWith([mineResponsible]);
    when(() => repo.boardChanges).thenAnswer((_) => changes.stream);
    await openMine(tester, repo);
    verify(() => repo.myTickets(projectId: null, includeDone: false)).called(1);
    changes.add(const Event(aggregateId: 'k1', atMs: 0, kind: EventKind.ticketFollowChanged, seq: 9));
    await settleBoard(tester);
    verify(() => repo.myTickets(projectId: null, includeDone: false)).called(1);
  });
}
