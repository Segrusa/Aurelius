import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/board_settings_page.dart';
import 'package:aurelius_mobile/features/board/dispatcher_row.dart';
import 'package:aurelius_mobile/features/board/new_ticket_page.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  group('New ticket (§10.2, MU1-07)', () {
    Future<MockRepository> open(WidgetTester tester, {String? projectId, CompanyState? company}) async {
      final repo = boardRepository();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultTicketCreated(number: 52, ticketId: 'k52')));
      await pumpBoardPage(tester, repo, NewTicketPage(projectId: projectId), company: company);
      return repo;
    }

    final twoProjects = companyWith(projects: [
      ProjectSummary(createdAtMs: 0, id: 'p1', name: 'Atlas', status: ProjectStatus.active, version: 1, employeesActive: 0),
      ProjectSummary(createdAtMs: 0, id: 'p2', name: 'Borealis', status: ProjectStatus.active, version: 1, employeesActive: 0),
    ]);

    testWidgets('title and project are required: nothing is sent and the missing field says so (AC2)', (tester) async {
      final repo = await open(tester, company: twoProjects);
      await tester.tap(find.byKey(const ValueKey('new-ticket-create')));
      await settleBoard(tester);
      expect(find.text('Required.'), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('new-ticket-title')), 'Login per board member');
      await tester.tap(find.byKey(const ValueKey('new-ticket-create')));
      await settleBoard(tester);
      expect(find.text('Choose a project.'), findsOneWidget);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
    });

    testWidgets('Create: Backlog, with priority, owner, labels and mode (AC1, B9)', (tester) async {
      final repo = await open(tester, projectId: 'p2', company: twoProjects);
      expect(find.text('Lands in Backlog. Nothing starts until you press Start. Created by ada.'), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('new-ticket-title')), '  Login per board member  ');
      await tester.enterText(find.byKey(const ValueKey('new-ticket-description')), 'Argon2 hashing.');
      await tester.tap(find.text('Urgent'));
      await tester.enterText(find.byKey(const ValueKey('new-ticket-labels')), 'backend, security');
      await tester.tap(find.byKey(const ValueKey('new-ticket-owner')));
      await settleBoard(tester);
      expect(find.byKey(const ValueKey('new-owner-dispatcher')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('new-owner-forge')));
      await settleBoard(tester);
      await tester.ensureVisible(find.byKey(const ValueKey('new-ticket-advanced')));
      await tester.tap(find.byKey(const ValueKey('new-ticket-advanced')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('new-ticket-mode-plan')));
      await settleBoard(tester);
      expect(find.text('Stops in Plan review with a plan before anything changes.'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('new-ticket-create')));
      await settleBoard(tester);
      final create = writesOf(repo).single;
      expect(create['method'], 'ticket.create');
      expect(paramsOf(create), {
        'title': 'Login per board member',
        'project_id': 'p2',
        'description': 'Argon2 hashing.',
        'priority': 'urgent',
        'owner_employee_id': 'forge',
        'labels': ['backend', 'security'],
        'files': <String>[],
        'run': {'mode': 'plan'},
      });
      expect(find.text('#52 created in Backlog.'), findsOneWidget);
    });

    testWidgets('Create and start: one deliberate click that says start (D-09)', (tester) async {
      final repo = await open(tester);
      await tester.enterText(find.byKey(const ValueKey('new-ticket-title')), 'Columns');
      await tester.ensureVisible(find.byKey(const ValueKey('new-ticket-create-start')));
      await tester.tap(find.byKey(const ValueKey('new-ticket-create-start')));
      await settleBoard(tester);
      final create = writesOf(repo).single;
      expect(paramsOf(create), containsPair('start', true));
      expect(paramsOf(create), containsPair('project_id', 'p1'), reason: 'the only open project');
      expect(paramsOf(create).containsKey('priority'), isFalse, reason: 'Normal is the default');
    });
  });

  group('Settings → Board (§12.1, MU1-06, B3)', () {
    testWidgets('the six columns with their role; rename sends all six in order with the board version (AC1, AC4)', (tester) async {
      final repo = boardRepository(board: boardOf(const [], versions: {ColumnKind.review: 5}));
      await pumpBoardPage(tester, repo, const BoardSettingsPage());
      for (final role in ['Start', 'Dispatch', 'Work', 'Plan gate · Used only in Plan mode', 'Review gate', 'Closed']) {
        expect(find.text(role), findsOneWidget);
      }
      expect(find.byIcon(Icons.delete), findsNothing, reason: 'a column with a role cannot be removed (AC2)');
      await tester.tap(find.text('Review'));
      await settleBoard(tester);
      await tester.enterText(find.byKey(const ValueKey('column-name')), 'Backlog');
      await tester.pump();
      expect(find.text('Another column has this name.'), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('column-name')), '');
      await tester.pump();
      expect(find.text('A column needs a name.'), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('column-name')), 'Check');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('column-name-save')));
      await settleBoard(tester);
      final update = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: 5, retry: false)).captured.single as Map<String, dynamic>;
      expect(update['method'], 'board.columns_update');
      expect((paramsOf(update)['columns'] as List).map((c) => (c as Map)['name']), ['Backlog', 'Todo', 'Doing', 'Plan review', 'Check', 'Done']);
      expect((paramsOf(update)['columns'] as List).map((c) => (c as Map)['column_id']), [for (final (k, _) in defaultColumns) columnIdOf(k)]);
    });

    testWidgets('reorder: the new order goes as all six with the version', (tester) async {
      final repo = boardRepository(board: boardOf(const [], versions: {ColumnKind.done: 2}));
      await pumpBoardPage(tester, repo, const BoardSettingsPage());
      expect(find.descendant(of: find.byKey(const ValueKey('column-col-done')), matching: find.byIcon(Icons.drag_handle)), findsOneWidget, reason: 'a drag handle per row');
      // Done goes above Review (the list gives the index once Done is out of its place).
      tester.widget<ReorderableListView>(find.byKey(const ValueKey('board-columns'))).onReorderItem!(5, 4);
      await settleBoard(tester);
      final update = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: 2, retry: false)).captured.single as Map<String, dynamic>;
      expect((paramsOf(update)['columns'] as List).map((c) => (c as Map)['name']), ['Backlog', 'Todo', 'Doing', 'Plan review', 'Done', 'Review']);
    });

    testWidgets('the default dispatcher: company.update with the company version; None is allowed (B5)', (tester) async {
      final repo = boardRepository(board: boardOf(const []));
      final company = MockCompanyBloc();
      await pumpBoardPage(tester, repo, const BoardSettingsPage(), companyBloc: company);
      expect(find.text('Orion'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('company-dispatcher')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('dispatcher-none')));
      await settleBoard(tester);
      final sent = verify(() => company.send(captureAny(), expectedVersion: 4, retry: false, done: any(named: 'done'))).captured.single as Map<String, dynamic>;
      expect(sent['method'], 'company.update');
      expect(paramsOf(sent), {'dispatcher': <String, dynamic>{}});
    });
  });

  group("a project's dispatcher (§12.2, MU1-11)", () {
    Widget row() => Scaffold(body: ProjectDispatcherRow(project: ProjectSummary(createdAtMs: 0, id: 'p1', name: 'Atlas', status: ProjectStatus.active, version: 2, employeesActive: 0)));

    testWidgets('choose its own (project.update with the project version) (AC1)', (tester) async {
      final company = MockCompanyBloc();
      await pumpBoardPage(tester, boardRepository(), row(), companyBloc: company);
      expect(find.text('Company default (Orion)'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('project-dispatcher')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('project-dispatcher-forge')));
      await settleBoard(tester);
      final sent = verify(() => company.send(captureAny(), expectedVersion: 2, retry: false, done: any(named: 'done'))).captured.single as Map<String, dynamic>;
      expect(sent['method'], 'project.update');
      expect(paramsOf(sent), {'project_id': 'p1', 'dispatcher': {'employee_id': 'forge'}});
    });

    testWidgets('an archived one does not pass to the company: nobody chooses, and it says so (AC3, B5 strict)', (tester) async {
      final gone = employeeOf(id: 'gone', name: 'Vesta', status: LifecycleStatus.archived);
      await pumpBoardPage(
        tester,
        boardRepository(),
        Scaffold(body: ProjectDispatcherRow(project: ProjectSummary(createdAtMs: 0, id: 'p1', name: 'Atlas', status: ProjectStatus.active, version: 2, employeesActive: 0, dispatcherEmployeeId: 'gone'))),
        company: companyWith(employees: [orion, forge, gone], projectDispatcher: 'gone'),
      );
      expect(find.text('◆ Vesta is archived. Started tickets wait in Todo until you choose an owner; the company default does not take over.'), findsOneWidget);
      expect(find.textContaining('Tickets go to the company default'), findsNothing);
    });

    testWidgets('a paused one does not pass to the company either (B5 strict)', (tester) async {
      final resting = employeeOf(id: 'resting', name: 'Vesta', status: LifecycleStatus.paused);
      await pumpBoardPage(
        tester,
        boardRepository(),
        Scaffold(body: ProjectDispatcherRow(project: ProjectSummary(createdAtMs: 0, id: 'p1', name: 'Atlas', status: ProjectStatus.active, version: 2, employeesActive: 0, dispatcherEmployeeId: 'resting'))),
        company: companyWith(employees: [orion, forge, resting], projectDispatcher: 'resting'),
      );
      expect(find.text('◆ Vesta is paused. Started tickets wait in Todo until you choose an owner; the company default does not take over.'), findsOneWidget);
    });

    testWidgets("the company default, archived: nobody chooses (B5)", (tester) async {
      final gone = employeeOf(id: 'gone', name: 'Vesta', status: LifecycleStatus.archived);
      await pumpBoardPage(tester, boardRepository(), row(), company: companyWith(dispatcher: 'gone', employees: [orion, forge, gone]));
      expect(find.text('◆ Vesta is archived. Started tickets wait in Todo until you choose an owner.'), findsOneWidget);
    });

    testWidgets('nobody at all: started tickets wait for you (B5)', (tester) async {
      await pumpBoardPage(tester, boardRepository(), row(), company: companyWith(dispatcher: null));
      expect(find.text('No dispatcher. Started tickets wait in Todo until you choose an owner.'), findsOneWidget);
    });
  });
}
