import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/widgets/ios.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/company/projects_page.dart';
import 'package:aurelius_mobile/features/company_org/company_org_page.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/dashboard/dashboard_page.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

EmployeeSummary employee(String id, String name, {bool working = false, String? reportsTo}) =>
    employeeOf(id: id, name: name, role: 'Lead', conversationId: 'c-$id', working: working, reportsTo: reportsTo);

TaskView task(String id, String employeeId, TaskStatus status, {String projectId = 'p'}) =>
    taskOf(id: id, projectId: projectId, employeeId: employeeId, title: 'Task $id', goal: '', status: status);

Future<MockCompanyBloc> show(WidgetTester tester, CompanyState state, Widget page) async {
  final bloc = MockCompanyBloc();
  final link = MockConnectionBloc();
  whenListen(link, const Stream<LinkState>.empty(), initialState: const LinkState(status: LinkStatus.online));
  whenListen(bloc, const Stream<CompanyState>.empty(), initialState: state);
  final router = GoRouter(routes: [
    GoRoute(path: '/', builder: (_, _) => MultiBlocProvider(providers: [BlocProvider<CompanyBloc>.value(value: bloc), BlocProvider<ConnectionBloc>.value(value: link)], child: page)),
    GoRoute(path: '/employee/:eid', builder: (_, s) => Scaffold(body: Text('employee ${s.pathParameters['eid']}'))),
    GoRoute(path: '/project/:pid', builder: (_, s) => Scaffold(body: Text('project ${s.pathParameters['pid']}'))),
    GoRoute(path: '/project/:pid/task/:tid', builder: (_, s) => Scaffold(body: Text('task ${s.pathParameters['tid']}'))),
  ]);
  await tester.pumpWidget(MaterialApp.router(theme: aureliusTheme(), routerConfig: router));
  await tester.pump(const Duration(milliseconds: 100));
  return bloc;
}

void main() {
  setUpAll(() => registerFallbackValue(const CompanyRefreshed()));
  final project = projectOf(id: 'p', name: 'Atlas');

  group('Home', () {
    testWidgets('shows Give work with the single root preselected, the management team and who else is at work', (tester) async {
      tester.view.physicalSize = const Size(1200, 2400);
      addTearDown(tester.view.resetPhysicalSize);
      await show(
        tester,
        CompanyState(
          loading: false,
          projects: [project],
          employees: [employee('ceo', 'Ada'), employee('b', 'Bo', working: true, reportsTo: 'ceo'), employee('c', 'Cy', reportsTo: 'ceo')],
          tasks: [task('1', 'b', TaskStatus.running)],
        ),
        const DashboardPage(),
      );
      expect(find.byKey(const ValueKey('give-work')), findsOneWidget);
      // The only root leads by default; the only project is the project.
      expect(find.descendant(of: find.byKey(const ValueKey('give-work-lead')), matching: find.text('Ada')), findsOneWidget);
      expect(find.descendant(of: find.byKey(const ValueKey('give-work-project')), matching: find.text('Atlas')), findsOneWidget);
      expect(find.text('Management Team'), findsOneWidget);
      expect(find.byKey(const ValueKey('team-ceo')), findsOneWidget);
      expect(find.byKey(const ValueKey('team-b')), findsNothing, reason: 'Bo reports to Ada');
      expect(find.textContaining('At Work'), findsOneWidget);
      expect(find.text('Task 1 · Atlas'), findsOneWidget);
      expect(find.byKey(const ValueKey('at-work-c')), findsNothing, reason: 'Cy is not working');
      await tester.tap(find.byKey(const ValueKey('at-work-b')));
      await tester.pumpAndSettle();
      expect(find.text('employee b'), findsOneWidget);
    });

    testWidgets('with several roots nobody is preselected, and Give stays off until there is a title', (tester) async {
      await show(
        tester,
        CompanyState(loading: false, projects: [project], employees: [employee('a', 'Ada'), employee('b', 'Bo')]),
        const DashboardPage(),
      );
      expect(find.text('Choose who leads'), findsOneWidget);
      FilledButton give() => tester.widget<FilledButton>(find.byKey(const ValueKey('give-work-send')));
      expect(give().onPressed, isNull);
      await tester.enterText(find.byKey(const ValueKey('give-work-title')), 'Write the launch post');
      await tester.pump();
      expect(give().onPressed, isNotNull);
    });

    testWidgets('Needs you lists waiting tasks and opens them', (tester) async {
      await show(
        tester,
        CompanyState(
          loading: false,
          projects: [project],
          employees: [employee('a', 'Ada')],
          tasks: [taskOf(id: 'w', projectId: 'p', employeeId: 'a', title: 'Pick a name', goal: '', status: TaskStatus.waiting, waitingReason: 'input')],
        ),
        const DashboardPage(),
      );
      expect(find.textContaining('Needs You'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('home-waiting-w')));
      await tester.pumpAndSettle();
      expect(find.text('task w'), findsOneWidget);
    });
  });

  group('Projects', () {
    testWidgets('rows show status, who works there and counts, and open on tap', (tester) async {
      // Rows are built lazily: make room for all four.
      tester.view.physicalSize = const Size(1200, 2400);
      addTearDown(tester.view.resetPhysicalSize);
      await show(
        tester,
        CompanyState(
          loading: false,
          projects: [
            projectOf(id: 'p', name: 'Atlas'),
            projectOf(id: 'q', name: 'Borealis'),
            projectOf(id: 'r', name: 'Cirrus', status: ProjectStatus.paused),
            projectOf(id: 's', name: 'Delta'),
          ],
          employees: [employee('a', 'Ada', working: true), employee('b', 'Bo')],
          tasks: [
            task('1', 'a', TaskStatus.running),
            task('2', 'a', TaskStatus.succeeded),
            taskOf(id: '3', projectId: 'q', employeeId: 'b', title: 'Task 3', goal: '', status: TaskStatus.waiting, pendingApprovals: 2),
          ],
        ),
        const ProjectsPage(),
      );
      expect(find.byType(Card), findsNothing);
      Finder inRow(String id, String text) => find.descendant(of: find.byKey(ValueKey('project-$id')), matching: find.text(text));
      expect(inRow('p', 'Working'), findsOneWidget);
      expect(inRow('p', '1 employee active · Ada · 1 working · 1 active task'), findsOneWidget);
      expect(inRow('q', 'Needs you · 1'), findsOneWidget);
      expect(inRow('q', '1 employee active · Bo · 1 active task'), findsOneWidget);
      expect(inRow('r', 'Paused'), findsOneWidget);
      expect(inRow('s', 'Quiet'), findsOneWidget);
      expect(inRow('s', 'Nobody at work here'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('project-q')));
      await tester.pumpAndSettle();
      expect(find.text('project q'), findsOneWidget);
    });

    testWidgets('Holding a project row and dragging it reorders the projects', (tester) async {
      final bloc = await show(
        tester,
        CompanyState(loading: false, projects: [projectOf(id: 'p', name: 'Atlas'), projectOf(id: 'q', name: 'Borealis'), projectOf(id: 'r', name: 'Cirrus')]),
        const ProjectsPage(),
      );
      final events = <CompanyEvent>[];
      when(() => bloc.add(any())).thenAnswer((i) => events.add(i.positionalArguments.first as CompanyEvent));
      final row = find.byKey(const ValueKey('project-p'));
      final drag = await tester.startGesture(tester.getCenter(row));
      await tester.pump(kLongPressTimeout + const Duration(milliseconds: 100));
      for (var i = 0; i < 10; i++) {
        await drag.moveBy(Offset(0, tester.getSize(row).height / 4));
        await tester.pump(const Duration(milliseconds: 50));
      }
      await drag.up();
      await tester.pumpAndSettle();
      final reordered = events.whereType<ProjectsReordered>().single;
      expect((reordered.from, reordered.to), (0, 2));
      expect(find.text('project p'), findsNothing, reason: 'a drag does not open the project');
    });
  });

  group('Company', () {
    final state = CompanyState(
      loading: false,
      projects: [project],
      employees: [
        employee('ceo', 'Ada'),
        employee('b', 'Bo', reportsTo: 'ceo'),
        employee('c', 'Cy', reportsTo: 'b'),
        employee('e', 'Eve', reportsTo: 'ceo'),
      ],
      proposals: [proposalOf('pr1', name: 'Sam', reportsTo: 'ceo', reportsToName: 'Ada')],
    );

    testWidgets('Employees: proposals, then the management team, then the others', (tester) async {
      tester.view.physicalSize = const Size(1200, 2400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await show(tester, state, const CompanyOrgPage());
      expect(find.byKey(const ValueKey('proposal-pr1')), findsOneWidget);
      expect(find.textContaining('reports to Ada'), findsOneWidget);
      final headers = ['Proposals', 'Management team', 'Employees'].map((t) => tester.getTopLeft(find.textContaining(RegExp('^$t  \\d'))).dy).toList();
      expect(headers, orderedEquals([...headers]..sort()), reason: 'proposals, then the management team, then the others');
      expect(find.byKey(const ValueKey('employee-e')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('employee-c')));
      await tester.pumpAndSettle();
      expect(find.text('employee c'), findsOneWidget);
    });

    testWidgets('Org chart: one company-wide tree, no Follow-the-org-chart switch', (tester) async {
      tester.view.physicalSize = const Size(1200, 2400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await show(tester, state, const CompanyOrgPage(initial: 'chart'));
      expect(find.byKey(const ValueKey('follow-org-chart')), findsNothing);
      double x(String id) => tester.getTopLeft(find.descendant(of: find.byKey(ValueKey('org-node-$id')), matching: find.byType(Text)).first).dx;
      double y(String id) => tester.getTopLeft(find.byKey(ValueKey('org-node-$id'))).dy;
      expect(x('ceo') < x('b') && x('b') < x('c'), isTrue, reason: 'each level is indented');
      expect(y('ceo') < y('b') && y('b') < y('c') && y('c') < y('e'), isTrue, reason: 'reports follow their boss, by name');
    });

    testWidgets('only Employees and Org chart; an unknown view opens Employees', (tester) async {
      tester.view.physicalSize = const Size(1200, 2400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await show(tester, state, const CompanyOrgPage(initial: 'board'));
      expect(find.byType(PillSegments<CompanyOrgView>), findsOneWidget);
      expect(tester.widget<PillSegments<CompanyOrgView>>(find.byType(PillSegments<CompanyOrgView>)).segments.map((s) => s.$2), ['Employees', 'Org chart']);
      expect(find.byKey(const ValueKey('employee-e')), findsOneWidget);
      expect(find.byKey(const ValueKey('org-chart')), findsNothing);
    });
  });
}
