import 'package:aurelius_mobile/core/net/connection.dart' show LinkState, LinkStatus;
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/quick_task.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/new_ticket_page.dart';
import 'package:aurelius_mobile/features/dashboard/dashboard_page.dart' show GiveWorkBox;
import 'package:aurelius_mobile/features/dashboard/give_work.dart';
import 'package:flutter/material.dart';
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
}
