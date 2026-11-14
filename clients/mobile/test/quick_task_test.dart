import 'package:bloc_test/bloc_test.dart';
import 'package:aurelius_mobile/core/net/connection.dart' show LinkState, LinkStatus;
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/quick_task.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/board_bloc.dart';
import 'package:aurelius_mobile/features/board/board_page.dart';
import 'package:aurelius_mobile/features/board/my_tickets_bloc.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/board/new_ticket_page.dart';
import 'package:aurelius_mobile/features/dashboard/dashboard_page.dart' show GiveWorkBox;
import 'package:aurelius_mobile/features/dashboard/give_work.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// Ticket #53 (ST-01, ST-05, ST-07, BR4): the quick task on the phone. Nothing
// of it shows on a server without `quick_task`, and `kind` goes out only for a
// quick task, since an older server refuses fields it does not know.

/// [onlineLink] on a server with quick tasks.
const quickLink = LinkState(status: LinkStatus.online, capabilities: ['read', 'chat', 'task_control', 'approve'], features: [quickTaskFeature], boardMembersExist: true, user: testUser, claudeLogin: testLogin);

/// A server with My tickets, without and with quick tasks.
const myLink = LinkState(status: LinkStatus.online, capabilities: ['read', 'chat', 'task_control', 'approve'], features: ['my_tickets'], boardMembersExist: true, user: testUser, claudeLogin: testLogin);
const myQuickLink = LinkState(status: LinkStatus.online, capabilities: ['read', 'chat', 'task_control', 'approve'], features: ['my_tickets', quickTaskFeature], boardMembersExist: true, user: testUser, claudeLogin: testLogin);

final quickOne = ticketOf(id: 'q1', number: 7, column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge', title: 'Fix the footer', kind: quickKind);
final normalOne = ticketOf(id: 'n1', number: 8, column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge', title: 'Plan the release');

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  setUp(() {
    SharedPreferences.setMockInitialValues({});
    LastQuickEmployee.reset();
  });

  Future<void> tapKey(WidgetTester tester, String key) async {
    await tester.ensureVisible(find.byKey(ValueKey(key)));
    await tester.tap(find.byKey(ValueKey(key)));
    await settleBoard(tester);
  }

  group('the create request', () {
    test('a quick task sends kind quick, its employee and description, and never start', () {
      final params = paramsOf(quickTaskRequest(projectId: 'p1', description: '  Fix the typo\nin the footer ', employeeId: 'forge'));
      expect(params['kind'], 'quick');
      expect(params['owner_employee_id'], 'forge');
      expect(params['description'], 'Fix the typo\nin the footer');
      expect(params['title'], '');
      expect(params.containsKey('start'), isFalse);
    });

    test('Give work sends kind only for a quick task', () {
      final ticket = paramsOf(givenWorkTicket('p1', 'Write the notes', 'forge'));
      expect(ticket.containsKey('kind'), isFalse);
      expect(ticket['title'], 'Write the notes');
      expect(ticket['start'], isTrue);
      final quick = paramsOf(givenWorkTicket('p1', 'Fix the typo', 'forge', quick: true));
      expect(quick['kind'], 'quick');
      expect(quick['description'], 'Fix the typo');
      expect(quick['title'], '');
      expect(quick.containsKey('start'), isFalse);
    });
  });

  group('New ticket', () {
    Future<MockRepository> open(WidgetTester tester, {LinkState link = quickLink, TicketDraft? draft}) async {
      final repo = boardRepository(link: link);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultTicketCreated(number: 53, ticketId: 'k53')));
      await pumpBoardPage(tester, repo, NewTicketPage(projectId: 'p1', draft: draft), link: link);
      return repo;
    }

    testWidgets('without the feature there is no Quick task choice, and a ticket sends no kind', (tester) async {
      final repo = await open(tester, link: onlineLink);
      expect(find.byKey(const ValueKey('new-ticket-kind')), findsNothing);
      expect(find.text(quickTaskName), findsNothing);
      await tester.enterText(find.byKey(const ValueKey('new-ticket-title')), 'A title');
      await tapKey(tester, 'new-ticket-create');
      expect(paramsOf(writesOf(repo).single).containsKey('kind'), isFalse);
    });

    testWidgets('with the feature a normal ticket still sends no kind', (tester) async {
      final repo = await open(tester);
      expect(find.byKey(const ValueKey('new-ticket-kind')), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('new-ticket-title')), 'A title');
      await tapKey(tester, 'new-ticket-create');
      expect(paramsOf(writesOf(repo).single).containsKey('kind'), isFalse);
    });

    testWidgets('a quick task needs a description and an employee, not a title', (tester) async {
      final repo = await open(tester);
      await tester.tap(find.text(quickTaskName));
      await settleBoard(tester);
      expect(find.byKey(const ValueKey('new-ticket-create-start')), findsNothing);
      expect(find.text('Title *'), findsNothing);
      await tapKey(tester, 'new-ticket-create');
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
      expect(find.text('Required.'), findsOneWidget);
      expect(find.byKey(const ValueKey('new-ticket-owner-error')), findsOneWidget);

      await tester.enterText(find.byKey(const ValueKey('new-ticket-description')), 'Fix the typo in the footer');
      await tapKey(tester, 'new-ticket-owner');
      expect(find.byKey(const ValueKey('new-owner-dispatcher')), findsNothing);
      await tester.tap(find.byKey(const ValueKey('new-owner-forge')));
      await settleBoard(tester);
      await tapKey(tester, 'new-ticket-create');
      final params = paramsOf(writesOf(repo).single);
      expect(params['kind'], 'quick');
      expect(params['owner_employee_id'], 'forge');
      expect(params['description'], 'Fix the typo in the footer');
      expect(params['title'], '');
      expect(params.containsKey('start'), isFalse);
      expect(LastQuickEmployee.employee, 'forge');
    });

    testWidgets('a created quick task opens on its discussion right away (ticket #58)', (tester) async {
      final repo = boardRepository(link: quickLink);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultTicketCreated(number: 53, ticketId: 'k53')));
      final router = await pumpBoardPage(tester, repo, const NewTicketPage(projectId: 'p1'), link: quickLink);
      await tester.tap(find.text(quickTaskName));
      await settleBoard(tester);
      await tester.enterText(find.byKey(const ValueKey('new-ticket-description')), 'Fix the typo in the footer');
      await tapKey(tester, 'new-ticket-owner');
      await tester.tap(find.byKey(const ValueKey('new-owner-forge')));
      await settleBoard(tester);
      await tapKey(tester, 'new-ticket-create');
      expect(router.state.uri.toString(), '/ticket/k53?tab=discussion');
    });

    testWidgets('the employee last given a quick task is filled in', (tester) async {
      SharedPreferences.setMockInitialValues({'quick.last.h': 'forge'});
      await open(tester);
      await tester.tap(find.text(quickTaskName));
      await settleBoard(tester);
      expect(find.descendant(of: find.byKey(const ValueKey('new-ticket-owner')), matching: find.text('Forge')), findsOneWidget);
    });

    testWidgets('Give work opens it as a quick task with the text as description', (tester) async {
      await open(tester, draft: const TicketDraft(title: '', description: 'Rename the button', projectId: 'p1', ownerId: 'forge', quick: true));
      expect(find.text('New quick task'), findsOneWidget);
      expect(find.descendant(of: find.byKey(const ValueKey('new-ticket-owner')), matching: find.text('Forge')), findsOneWidget);
    });

    testWidgets('a server error shows as it comes', (tester) async {
      final repo = await open(tester);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenThrow(const ProtocolException(ErrorCode.validation, 'Dit project heeft geen git-map'));
      await tester.tap(find.text(quickTaskName));
      await settleBoard(tester);
      await tester.enterText(find.byKey(const ValueKey('new-ticket-description')), 'Fix it');
      await tapKey(tester, 'new-ticket-owner');
      await tester.tap(find.byKey(const ValueKey('new-owner-forge')));
      await settleBoard(tester);
      await tapKey(tester, 'new-ticket-create');
      expect(find.textContaining('Dit project heeft geen git-map'), findsOneWidget);
    });
  });

  group('Give work on Home', () {
    Future<MockRepository> open(WidgetTester tester, {LinkState link = quickLink}) async {
      final repo = boardRepository(link: link);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultTicketCreated(number: 54, ticketId: 'k54')));
      await pumpBoardPage(tester, repo, Scaffold(body: GiveWorkBox(state: companyWith())), link: link);
      return repo;
    }

    testWidgets('the ticket it makes opens on its discussion right away (ticket #58)', (tester) async {
      final repo = boardRepository(link: quickLink);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultTicketCreated(number: 54, ticketId: 'k54')));
      final router = await pumpBoardPage(tester, repo, Scaffold(body: GiveWorkBox(state: companyWith())), link: quickLink);
      await tester.enterText(find.byKey(const ValueKey('give-work-title')), 'Fix the typo');
      await tester.pump();
      await tapKey(tester, 'give-work-lead');
      await tester.tap(find.byKey(const ValueKey('give-lead-forge')));
      await settleBoard(tester);
      await tapKey(tester, 'give-work-send');
      expect(router.state.uri.toString(), '/ticket/k54?tab=discussion');
    });

    testWidgets('without the feature there is no Quick task chip', (tester) async {
      await open(tester, link: onlineLink);
      expect(find.byKey(const ValueKey('give-work-quick')), findsNothing);
    });

    testWidgets('a quick task waits for an employee, then sends kind quick with the text as description', (tester) async {
      final repo = await open(tester);
      await tapKey(tester, 'give-work-quick');
      await tester.enterText(find.byKey(const ValueKey('give-work-title')), 'Fix the typo');
      await tester.pump();
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('give-work-send'))).onPressed, isNull);
      await tapKey(tester, 'give-work-lead');
      await tester.tap(find.byKey(const ValueKey('give-lead-forge')));
      await settleBoard(tester);
      await tapKey(tester, 'give-work-send');
      final params = paramsOf(writesOf(repo).single);
      expect(params['kind'], 'quick');
      expect(params['description'], 'Fix the typo');
      expect(params['owner_employee_id'], 'forge');
      expect(LastQuickEmployee.employee, 'forge');
    });

    testWidgets('the employee last given a quick task is chosen again', (tester) async {
      SharedPreferences.setMockInitialValues({'quick.last.h': 'forge'});
      await open(tester);
      await tapKey(tester, 'give-work-quick');
      expect(find.descendant(of: find.byKey(const ValueKey('give-work-lead')), matching: find.text('Forge')), findsOneWidget);
    });
  });

  group('the label and the filter (ST-07)', () {
    Widget board(MockRepository repo) => BlocProvider(create: (_) => BoardBloc(repo), child: const BoardPage());

    MockRepository repoWith(List<TicketView> tickets, {LinkState link = quickLink}) {
      final repo = boardRepository(board: boardOf(tickets), link: link);
      when(() => repo.myTickets(projectId: any(named: 'projectId'), includeDone: any(named: 'includeDone'))).thenAnswer((_) async => MyTicketsView(tickets: tickets, following: const []));
      return repo;
    }

    blocTest<BoardBloc, BoardState>(
      'Quick tasks keeps only quick tasks in the columns',
      build: () => BoardBloc(repoWith([quickOne, normalOne])),
      act: (bloc) async {
        await stateWhere(bloc, (s) => s.board != null);
        bloc.add(const BoardQuickOnly(true));
      },
      wait: const Duration(milliseconds: 50),
      verify: (bloc) {
        expect(bloc.state.quickOnly, isTrue);
        expect([for (final t in bloc.state.tickets) t.id], ['q1']);
      },
    );

    test('My tickets keeps only quick tasks in every list with the filter', () {
      final state = MyTicketsState(loading: false, mine: MyTicketsView(tickets: [quickOne, normalOne], following: const []), quickOnly: true);
      expect([for (final t in state.inTab(MyTab.created, me: testUser.id, needsMe: (_) => false)) t.id], ['q1']);
      final all = MyTicketsState(loading: false, mine: MyTicketsView(tickets: [quickOne, normalOne], following: const []));
      expect(all.inTab(MyTab.created, me: testUser.id, needsMe: (_) => false), hasLength(2));
    });

    testWidgets('a quick task has the Quick tag on its card; a ticket does not', (tester) async {
      final repo = repoWith([quickOne, normalOne]);
      await pumpBoardPage(tester, repo, board(repo), link: quickLink);
      expect(find.byKey(const ValueKey('quick-tag-q1')), findsOneWidget);
      expect(find.byKey(const ValueKey('quick-tag-n1')), findsNothing);
      expect(find.descendant(of: find.byKey(const ValueKey('quick-tag-q1')), matching: find.text(quickLabel)), findsOneWidget);
    });

    testWidgets('without the feature there is no tag and no filter', (tester) async {
      final repo = repoWith([quickOne, normalOne], link: onlineLink);
      await pumpBoardPage(tester, repo, board(repo), link: onlineLink);
      expect(find.byKey(const ValueKey('quick-tag-q1')), findsNothing);
      expect(find.byKey(const ValueKey('board-filter-quick')), findsNothing);
      expect(find.text('Fix the footer'), findsOneWidget, reason: 'an older client shows it as a normal ticket');
    });

    testWidgets('the Board filter Quick tasks hides other tickets', (tester) async {
      final repo = repoWith([quickOne, normalOne]);
      await pumpBoardPage(tester, repo, board(repo), link: quickLink);
      expect(find.text('Plan the release'), findsOneWidget);
      await tapKey(tester, 'board-filter-quick');
      expect(find.text(quickFilterName), findsOneWidget);
      expect(find.text('Fix the footer'), findsOneWidget);
      expect(find.text('Plan the release'), findsNothing);
    });

    testWidgets('the Board filter with no quick task says so and clears', (tester) async {
      final repo = repoWith([normalOne]);
      await pumpBoardPage(tester, repo, board(repo), link: quickLink);
      await tapKey(tester, 'board-filter-quick');
      expect(find.text('No quick tasks.'), findsOneWidget);
      await tester.tap(find.text('Clear filter'));
      await settleBoard(tester);
      expect(find.text('Plan the release'), findsOneWidget);
    });

    testWidgets('My tickets has the filter Quick tasks with the feature only', (tester) async {
      final repo = repoWith([quickOne, normalOne], link: myQuickLink);
      await pumpBoardPage(tester, repo, board(repo), link: myQuickLink);
      await tester.tap(find.text('My tickets'));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('my-tab-created')));
      await settleBoard(tester);
      expect(find.text('Plan the release'), findsOneWidget);
      await tapKey(tester, 'my-filter-quick');
      expect(find.text('Fix the footer'), findsOneWidget);
      expect(find.text('Plan the release'), findsNothing);
      expect(find.byKey(const ValueKey('quick-tag-q1')), findsOneWidget);
    });

    testWidgets('My tickets without the feature has no filter Quick tasks', (tester) async {
      final repo = repoWith([quickOne, normalOne], link: myLink);
      await pumpBoardPage(tester, repo, board(repo), link: myLink);
      await tester.tap(find.text('My tickets'));
      await settleBoard(tester);
      expect(find.byKey(const ValueKey('my-filter-quick')), findsNothing);
    });
  });

  group('Review, Done and cancel (ST-05, BR4)', () {
    Future<MockRepository> open(WidgetTester tester, TicketView ticket, {LinkState link = quickLink}) async {
      final repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []), link: link);
      await pumpBoardPage(tester, repo, TicketPage(ticketId: ticket.id), link: link);
      return repo;
    }

    TicketView quickIn(ColumnKind column, {String? mergeCommit, int? doneAtMs}) =>
        ticketOf(id: 'q1', number: 9, column: column, ownerEmployeeId: 'forge', ownerName: 'Forge', title: 'Fix the footer', kind: quickKind, mergeCommit: mergeCommit, doneAtMs: doneAtMs);

    String barDone(WidgetTester tester) => tester.widget<Text>(find.descendant(of: find.byKey(const ValueKey('bar-done')), matching: find.byType(Text))).data!;

    testWidgets('in Review a quick task has Approve and merge; it asks, then sends ticket.accept', (tester) async {
      final repo = await open(tester, quickIn(ColumnKind.review));
      expect(barDone(tester), approveAndMerge);
      expect(find.byKey(const ValueKey('quick-tag-page')), findsOneWidget);
      await tapKey(tester, 'bar-done');
      expect(find.text('$approveAndMerge #9?'), findsOneWidget);
      expect(find.textContaining('Nothing is pushed.'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('mark-done')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.accept');
      expect(find.text('#9 is approved. Merging into main…'), findsOneWidget);
    });

    testWidgets('a normal ticket in Review keeps Done', (tester) async {
      await open(tester, ticketOf(column: ColumnKind.review, ownerEmployeeId: 'forge', ownerName: 'Forge'));
      expect(barDone(tester), 'Done');
    });

    testWidgets('without the feature a quick task keeps Done and shows no tag', (tester) async {
      await open(tester, quickIn(ColumnKind.review), link: onlineLink);
      expect(barDone(tester), 'Done');
      expect(find.byKey(const ValueKey('quick-tag-page')), findsNothing);
    });

    testWidgets('after Done the page shows the short merge commit', (tester) async {
      await open(tester, quickIn(ColumnKind.done, mergeCommit: '1a2b3c4d5e6f7a8b9c0d', doneAtMs: 0));
      expect(find.text("Merged into the project's main as 1a2b3c4. Nothing was pushed."), findsOneWidget);
    });

    testWidgets('after Done the card shows the short merge commit', (tester) async {
      final repo = boardRepository(board: boardOf([quickIn(ColumnKind.done, mergeCommit: '1a2b3c4d5e6f7a8b9c0d', doneAtMs: 0)]), link: quickLink);
      await pumpBoardPage(tester, repo, BlocProvider(create: (_) => BoardBloc(repo), child: const BoardPage()), link: quickLink);
      await tapKey(tester, 'board-chip-done');
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('card-done-q1')), findsOneWidget);
      expect(tester.widget<Text>(find.byKey(const ValueKey('card-done-q1'))).data, endsWith(' · merged 1a2b3c4'));
    });

    testWidgets('Back to Backlog of a quick task asks first; Cancel keeps it, confirming sends ticket.cancel', (tester) async {
      final repo = await open(tester, quickIn(ColumnKind.doing));
      await tapKey(tester, 'bar-backlog');
      expect(find.text(quickCancelTitle), findsOneWidget);
      expect(find.text(quickCancelBody), findsOneWidget);
      await tester.tap(find.widgetWithText(TextButton, 'Cancel'));
      await settleBoard(tester);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
      await tapKey(tester, 'bar-backlog');
      await tester.tap(find.widgetWithText(TextButton, 'Back to Backlog'));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'ticket.cancel');
      expect(find.text('#9 is back in Backlog. Its branch is removed.'), findsOneWidget);
    });

    testWidgets('a quick task in Review asks the same before it goes back to Backlog', (tester) async {
      await open(tester, quickIn(ColumnKind.review));
      await tester.tap(find.byTooltip('Ticket actions'));
      await settleBoard(tester);
      await tester.tap(find.text('Back to Backlog').last);
      await settleBoard(tester);
      expect(find.text(quickCancelTitle), findsOneWidget);
    });

    testWidgets('a normal ticket keeps its own question', (tester) async {
      await open(tester, ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge'));
      await tapKey(tester, 'bar-backlog');
      expect(find.text(quickCancelTitle), findsNothing);
      expect(find.text('Back to Backlog?'), findsOneWidget);
    });
  });
}
