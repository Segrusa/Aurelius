import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/company_org/company_org_page.dart';
import 'package:aurelius_mobile/features/company_org/employee_page.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/project/project_page.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

class MockRepository extends Mock implements AureliusRepository {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

const online = LinkState(status: LinkStatus.online, capabilities: ['read', 'chat', 'task_control', 'approve']);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  late MockRepository repo;
  var project = projectOf(id: 'p', name: 'Launch');
  var employees = <EmployeeSummary>[];
  var proposals = <ProposalView>[];
  var tasks = <TaskView>[];
  var follow = true;

  setUp(() {
    project = projectOf(id: 'p', name: 'Launch');
    employees = [];
    proposals = [];
    tasks = [];
    follow = true;
    repo = MockRepository();
    when(() => repo.host).thenReturn(const Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.changes).thenAnswer((_) => const Stream.empty());
    when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.linkState).thenReturn(online);
    when(() => repo.projects()).thenAnswer((_) async => [project]);
    when(() => repo.company()).thenAnswer((_) async => companyOf(employees: employees, followOrgChart: follow));
    when(() => repo.proposals()).thenAnswer((_) async => proposals);
    when(() => repo.tasks()).thenAnswer((_) async => tasks);
    when(() => repo.tasks(projectId: any(named: 'projectId'))).thenAnswer((_) async => tasks);
    when(() => repo.ideas(any())).thenAnswer((_) async => []);
    when(() => repo.memory(projectId: any(named: 'projectId'))).thenAnswer((_) async => []);
    when(() => repo.boardChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.board(projectId: any(named: 'projectId'))).thenAnswer((_) async => boardOf([ticketOf(projectId: 'p')], projectId: 'p'));
    when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
        .thenAnswer((_) async => receiptOf(const CommandResultFollowOrgChartSet(follow: true)));
  });

  Future<void> pumpPage(WidgetTester tester, Widget page) async {
    final connection = MockConnectionBloc();
    whenListen(connection, const Stream<LinkState>.empty(), initialState: online);
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: MultiBlocProvider(
        providers: [
          BlocProvider<ConnectionBloc>.value(value: connection),
          BlocProvider<CompanyBloc>(lazy: false, create: (_) => CompanyBloc(repo)),
        ],
        child: MaterialApp(theme: aureliusTheme(), home: page),
      ),
    ));
    await tester.pump(const Duration(milliseconds: 500));
  }

  Future<void> pump(WidgetTester tester) => pumpPage(tester, const ProjectPage(projectId: 'p'));

  Future<void> show(WidgetTester tester, String tab) async {
    DefaultTabController.of(tester.element(find.byType(TabBarView))).animateTo(ProjectPage.tabs.indexOf(tab));
    await tester.pumpAndSettle();
  }

  Finder inAppBar(Finder f) => find.descendant(of: find.byType(AppBar), matching: f);

  testWidgets('the + in the app bar belongs to the open tab, beside the project menu', (tester) async {
    await pump(tester);
    expect(find.text('Launch'), findsOneWidget);
    expect(inAppBar(find.byTooltip('Project actions')), findsOneWidget);

    // Overview adds nothing: only the menu.
    expect(inAppBar(find.byIcon(CupertinoIcons.add)), findsNothing);
    // Four tabs; employees are company-wide.
    expect(ProjectPage.tabs, ['overview', 'board', 'tasks', 'ideas']);
    expect(find.text('Conversations'), findsNothing);
    expect(find.text('Recurring'), findsNothing);
    expect(find.text('Notes'), findsNothing);
    expect(find.text('Timeline'), findsNothing);
    expect(find.text('Memory'), findsNothing);
    expect(find.text('Managers'), findsNothing);
    expect(find.text('Employees'), findsNothing);

    for (final (tab, tooltip) in [('board', 'New ticket'), ('tasks', 'New ticket'), ('ideas', 'New idea')]) {
      await show(tester, tab);
      expect(inAppBar(find.byKey(ValueKey('project-add-$tab'))), findsOneWidget, reason: tab);
      // The only add button of the tab is the one up there.
      expect(find.byTooltip(tooltip), findsOneWidget, reason: tab);
      expect(inAppBar(find.byTooltip(tooltip)), findsOneWidget, reason: tab);
      expect(find.byKey(const ValueKey('project-add-overview')), findsNothing);
      // The + sits right of the menu, in the same capsule.
      expect(tester.getCenter(find.byTooltip(tooltip)).dx, greaterThan(tester.getCenter(find.byTooltip('Project actions')).dx));
    }
    expect(find.text('New employee'), findsNothing, reason: 'employees are added on the Company tab');
    expect(tester.takeException(), isNull);
  });

  testWidgets('the Board tab shows only this project\'s tickets, without a project filter', (tester) async {
    await pump(tester);
    verifyNever(() => repo.board(projectId: any(named: 'projectId')));
    await show(tester, 'board');
    verify(() => repo.board(projectId: 'p')).called(1);
    verifyNever(() => repo.board(projectId: null));
    expect(find.text('Write the release notes'), findsOneWidget);
    expect(find.byKey(const ValueKey('board-count')), findsOneWidget);
    expect(find.byKey(const ValueKey('board-filter-project')), findsNothing, reason: 'the project is fixed');
    expect(find.byKey(const ValueKey('board-filter-owner')), findsOneWidget);
    expect(find.byTooltip('Board settings'), findsNothing, reason: 'the project page has the app bar');
    expect(tester.takeException(), isNull);
  });

  testWidgets('the overview shows who works here: employees with open tasks in the project', (tester) async {
    employees = [employeeOf(id: 'jean', name: 'Jean', role: 'Product'), employeeOf(id: 'tom', name: 'Tom', role: 'Tester', reportsTo: 'jean')];
    tasks = [
      taskOf(id: 't1', projectId: 'p', employeeId: 'tom', title: 'Test it', goal: '', status: TaskStatus.running),
      taskOf(id: 't2', projectId: 'p', employeeId: 'tom', title: 'Test more', goal: '', status: TaskStatus.queued),
      taskOf(id: 't3', projectId: 'p', employeeId: 'jean', title: 'Old', goal: '', status: TaskStatus.succeeded),
      taskOf(id: 't4', projectId: 'other', employeeId: 'jean', title: 'Elsewhere', goal: '', status: TaskStatus.running),
    ];
    await pump(tester);
    expect(find.textContaining('Who Works Here'), findsOneWidget);
    expect(find.byKey(const ValueKey('works-here-tom')), findsOneWidget);
    expect(find.text('Tester · 2 open tasks'), findsOneWidget);
    expect(find.byKey(const ValueKey('works-here-jean')), findsNothing, reason: 'Jean has only ended work here');
    // Let the company's debounced refresh finish before the tree goes.
    await tester.pump(const Duration(seconds: 1));
  });

  testWidgets('the + on Ideas notes an idea as before', (tester) async {
    await pump(tester);
    await show(tester, 'ideas');
    await tester.tap(find.byKey(const ValueKey('project-add-ideas')));
    await tester.pumpAndSettle();
    expect(find.text('New idea'), findsOneWidget);
    await tester.enterText(find.byType(TextField).first, 'Dark mode');
    await tester.pump();
    await tester.tap(find.text('Add'));
    await tester.pumpAndSettle();
    final command = writesOf(repo).single;
    expect(command['method'], 'idea.add');
    expect(paramsOf(command)['project_id'], 'p');
    expect(paramsOf(command)['title'], 'Dark mode');
    expect(tester.takeException(), isNull);
  });

  testWidgets('the project menu opens as a glass menu', (tester) async {
    await pump(tester);
    await tester.tap(find.byTooltip('Project actions'));
    await tester.pumpAndSettle();
    // Three entries: Edit…, Pause/Resume, Archive.
    expect(find.text('Edit…'), findsOneWidget);
    expect(find.text('Pause'), findsOneWidget);
    expect(find.text('Archive'), findsOneWidget);
    for (final gone in ['Documents', 'Artifacts', 'Agent functions', 'Settings', 'Rename']) {
      expect(find.text(gone), findsNothing, reason: gone);
    }
    expect(find.byType(PopupMenuItem<String>), findsNothing);
    // Tapping beside it closes it.
    await tester.tapAt(const Offset(20, 500));
    await tester.pumpAndSettle();
    expect(find.text('Edit…'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  group('company org chart', () {
    final jean = employeeOf(id: 'jean', name: 'Jean', role: 'Product', title: 'Head of product');
    final simon = employeeOf(id: 'simon', name: 'Simon', role: 'Server', reportsTo: 'jean');

    testWidgets('a proposed colleague can be approved', (tester) async {
      employees = [jean];
      proposals = [proposalOf('pr1', name: 'Sam', role: 'Designer', title: 'Lead designer', reportsTo: 'jean', reportsToName: 'Jean'), proposalOf('old', name: 'Done', status: ProposalStatus.approved)];
      await pumpPage(tester, const CompanyOrgPage());
      expect(find.text('Sam'), findsOneWidget);
      expect(find.text('Lead designer · Designer · reports to Jean'), findsOneWidget);
      expect(find.text('Proposed by Iris'), findsOneWidget);
      expect(find.byKey(const ValueKey('proposal-old')), findsNothing, reason: 'only pending proposals are asked about');
      await tester.tap(find.byKey(const ValueKey('approve-pr1')));
      await tester.pumpAndSettle();
      final command = writesOf(repo).single;
      expect(command['method'], 'proposal.decide');
      expect(paramsOf(command), {'approve': true, 'proposal_id': 'pr1'});
    });

    testWidgets('a proposed colleague can be declined with a note', (tester) async {
      employees = [jean];
      proposals = [proposalOf('pr1')];
      await pumpPage(tester, const CompanyOrgPage());
      await tester.tap(find.byKey(const ValueKey('decline-pr1')));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField).last, 'Not now');
      await tester.tap(find.widgetWithText(FilledButton, 'Decline'));
      await tester.pumpAndSettle();
      final command = writesOf(repo).single;
      expect(command['method'], 'proposal.decide');
      expect(paramsOf(command), {'approve': false, 'note': 'Not now', 'proposal_id': 'pr1'});
    });

    testWidgets('editing an employee sends its title and, apart, who it reports to; a cycle is shown as the server says it', (tester) async {
      employees = [simon, jean];
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((invocation) async {
        final request = invocation.positionalArguments.first as Map<String, dynamic>;
        if (request['method'] == 'employee.set_reports_to') throw const ProtocolException(ErrorCode.validation, 'Jean would report to an employee below Jean.');
        return receiptOf(const CommandResultEmployeeUpdated(employeeId: 'simon', version: 2));
      });
      tester.view.physicalSize = const Size(1200, 2400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await pumpPage(tester, const EmployeePage(employeeId: 'simon'));
      expect(find.text('Jean'), findsOneWidget, reason: 'Reports to');
      expect(find.byTooltip('Employee actions'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('employee-edit')));
      await tester.pumpAndSettle();
      expect(find.text('Reports to'), findsWidgets);
      await tester.enterText(find.widgetWithText(TextField, 'Title (optional)'), 'Head of server');
      expect(find.byType(DropdownButtonFormField<String>), findsOneWidget, reason: 'Reports to is the only choice');
      await tester.tap(find.byType(DropdownButtonFormField<String>));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Nobody (management team)').last);
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(FilledButton, 'Save'));
      await tester.pumpAndSettle();
      final sent = writesOf(repo);
      expect(sent.map((r) => r['method']), ['employee.update', 'employee.set_reports_to']);
      expect(paramsOf(sent.first)['title'], 'Head of server');
      expect(paramsOf(sent.last), {'employee_id': 'simon'});
      expect(find.text('Jean would report to an employee below Jean.'), findsOneWidget);
    });
  });
}
