import 'package:aurelius_mobile/app/shell.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/widgets/ios.dart';
import 'package:aurelius_mobile/data/company.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/more/more_page.dart';
import 'package:aurelius_mobile/features/project/new_task_page.dart';
import 'package:aurelius_mobile/features/tasks/tasks_page.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

import 'support/builders.dart';

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

void main() {
  final ceo = employeeOf(id: 'ceo', name: 'Ada', role: 'CEO');
  final cto = employeeOf(id: 'cto', name: 'Bo', role: 'CTO', reportsTo: 'ceo', departmentId: 'eng');
  final dev = employeeOf(id: 'dev', name: 'Cy', role: 'Developer', reportsTo: 'cto', departmentId: 'eng');
  final designer = employeeOf(id: 'des', name: 'Di', role: 'Designer', reportsTo: 'ceo', departmentId: 'design');
  final loose = employeeOf(id: 'loose', name: 'Ed', role: 'Writer', reportsTo: 'ceo');
  final gone = employeeOf(id: 'gone', name: 'Old', reportsTo: null, status: LifecycleStatus.archived);
  final departments = [departmentOf(id: 'eng', name: 'Engineering'), departmentOf(id: 'design', name: 'Design')];
  final everyone = [dev, loose, designer, cto, ceo, gone];

  group('company helpers', () {
    test('the management team is the active roots; a boss outside the company counts as none', () {
      expect(managementTeam(everyone).map((e) => e.id), ['ceo']);
      expect(managementTeam([employeeOf(id: 'x', name: 'X', reportsTo: 'archived-boss'), ceo]).map((e) => e.id), ['ceo', 'x']);
    });

    test('the single root is preselected; several roots or none preselect nobody', () {
      expect(defaultAssignee(everyone), 'ceo');
      expect(defaultAssignee([ceo, employeeOf(id: 'coo', name: 'Zed')]), isNull);
      expect(defaultAssignee(const []), isNull);
      expect(defaultAssignee([employeeOf(id: 'p', name: 'P', status: LifecycleStatus.paused)]), isNull, reason: 'a paused root does not take new work by default');
    });

    test('management is the root and its direct reports, not deeper or archived', () {
      expect(isManagement(everyone, 'ceo'), isTrue);
      expect(isManagement(everyone, 'cto'), isTrue);
      expect(isManagement(everyone, 'dev'), isFalse);
      expect(isManagement(everyone, 'gone'), isFalse);
    });

    test('pickers list the management team first, then departments by name, then no department', () {
      final groups = pickerGroups(everyone, departments);
      expect(groups.map((g) => g.title), ['Management team', 'Design', 'Engineering', 'No department']);
      expect(groups[2].employees.map((e) => e.id), ['cto', 'dev']);
      expect(groups.last.employees.map((e) => e.id), ['loose']);
      expect(groups.expand((g) => g.employees).map((e) => e.id), isNot(contains('gone')));
    });

    test('who works in a project: open tasks there, the busiest first', () {
      final tasks = [
        taskOf(id: '1', projectId: 'p', employeeId: 'dev', title: '', goal: '', status: TaskStatus.running),
        taskOf(id: '2', projectId: 'p', employeeId: 'dev', title: '', goal: '', status: TaskStatus.queued),
        taskOf(id: '3', projectId: 'p', employeeId: 'cto', title: '', goal: '', status: TaskStatus.waiting),
        taskOf(id: '4', projectId: 'p', employeeId: 'ceo', title: '', goal: '', status: TaskStatus.succeeded),
        taskOf(id: '5', projectId: 'q', employeeId: 'des', title: '', goal: '', status: TaskStatus.running),
      ];
      expect(workingIn('p', everyone, tasks).map((e) => e.id), ['dev', 'cto']);
      expect(projectsOf('des', tasks), ['q']);
    });
  });

  group('new task', () {
    Future<NewTask?> open(WidgetTester tester, {List<ProjectSummary>? projects, List<EmployeeSummary>? employees}) async {
      NewTask? result;
      await tester.pumpWidget(MaterialApp(
        theme: aureliusTheme(),
        home: Builder(
          builder: (context) => TextButton(
            onPressed: () async => result = await NewTaskPage.open(context, employees ?? everyone, departments: departments, projects: projects),
            child: const Text('open'),
          ),
        ),
      ));
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      return result;
    }

    ChoiceChip chip(WidgetTester tester, String key) => tester.widget<ChoiceChip>(find.byKey(ValueKey(key)));

    testWidgets('who leads: the management team first with the single root preselected', (tester) async {
      tester.view.physicalSize = const Size(1200, 3000);
      addTearDown(tester.view.resetPhysicalSize);
      await open(tester);
      expect(find.text('WHO LEADS THIS TASK?'), findsOneWidget);
      double top(String text) => tester.getTopLeft(find.text(text)).dy;
      expect(top('Management team'), lessThan(top('Design')));
      expect(top('Design'), lessThan(top('Engineering')));
      expect(top('Engineering'), lessThan(top('No department')));
      expect(chip(tester, 'lead-ceo').selected, isTrue);
      expect(chip(tester, 'lead-cto').selected, isFalse);
      expect(find.byKey(const ValueKey('lead-gone')), findsNothing, reason: 'archived employees cannot lead');
      expect(find.text('PROJECT'), findsNothing, reason: 'opened in a project, the project is known');
    });

    testWidgets('from Home the project is a field; Create waits for a project when there are several', (tester) async {
      tester.view.physicalSize = const Size(1200, 3000);
      addTearDown(tester.view.resetPhysicalSize);
      NewTask? result;
      await tester.pumpWidget(MaterialApp(
        theme: aureliusTheme(),
        home: Builder(
          builder: (context) => TextButton(
            onPressed: () async => result = await NewTaskPage.open(
              context,
              everyone,
              departments: departments,
              projects: [projectOf(id: 'p', name: 'Atlas'), projectOf(id: 'q', name: 'Borealis'), projectOf(id: 'x', name: 'Old', status: ProjectStatus.archived)],
            ),
            child: const Text('open'),
          ),
        ),
      ));
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      expect(find.text('PROJECT'), findsOneWidget);
      expect(find.byKey(const ValueKey('new-task-project-x')), findsNothing, reason: 'archived projects take no work');
      await tester.enterText(find.widgetWithText(TextField, 'Title'), 'Plan the launch');
      await tester.pump();
      FilledButton create() => tester.widget<FilledButton>(find.widgetWithText(FilledButton, 'Create'));
      expect(create().onPressed, isNull, reason: 'no project chosen yet');
      await tester.tap(find.byKey(const ValueKey('new-task-project-q')));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('lead-cto')));
      await tester.pump();
      expect(create().onPressed, isNotNull);
      await tester.tap(find.widgetWithText(FilledButton, 'Create'));
      await tester.pumpAndSettle();
      expect((result?.title, result?.projectId, result?.employeeId), ('Plan the launch', 'q', 'cto'));
    });

    testWidgets('with one open project it is preselected', (tester) async {
      await open(tester, projects: [projectOf(id: 'p', name: 'Atlas')]);
      expect(chip(tester, 'new-task-project-p').selected, isTrue);
    });
  });

  test('the bottom tabs: Home, Inbox, Tasks, More; Projects, Ideas, Company and Settings are under More', () {
    expect(shellTabs.map((t) => t.label), ['Home', 'Inbox', 'Tasks', 'More']);
  });

  group('company-wide pages', () {
    // Working tasks keep their mark breathing: step time instead of settling.
    Future<void> settle(WidgetTester tester) async {
      for (var i = 0; i < 10; i++) {
        await tester.pump(const Duration(milliseconds: 100));
      }
    }

    Future<void> show(WidgetTester tester, CompanyState state, Widget page) async {
      final bloc = MockCompanyBloc();
      whenListen(bloc, const Stream<CompanyState>.empty(), initialState: state);
      final router = GoRouter(routes: [
        GoRoute(path: '/', builder: (_, _) => BlocProvider<CompanyBloc>.value(value: bloc, child: page)),
        GoRoute(path: '/company', builder: (_, _) => Scaffold(appBar: AppBar(), body: const Text('company page'))),
        GoRoute(path: '/projects', builder: (_, _) => Scaffold(appBar: AppBar(), body: const Text('projects page'))),
        GoRoute(path: '/ideas', builder: (_, _) => Scaffold(appBar: AppBar(), body: const Text('ideas page'))),
        GoRoute(path: '/settings', builder: (_, _) => const Scaffold(body: Text('settings page'))),
        GoRoute(path: '/project/:pid/task/:tid', builder: (_, s) => Scaffold(body: Text('task ${s.pathParameters['tid']}'))),
      ]);
      await tester.pumpWidget(MaterialApp.router(theme: aureliusTheme(), routerConfig: router));
      await tester.pump(const Duration(milliseconds: 100));
    }

    final state = CompanyState(
      loading: false,
      projects: [projectOf(id: 'p', name: 'Atlas'), projectOf(id: 'q', name: 'Borealis')],
      employees: everyone,
      departments: departments,
      tasks: [
        taskOf(id: 'a', projectId: 'p', employeeId: 'dev', employeeName: 'Cy', title: 'Build it', goal: '', status: TaskStatus.running, updatedAtMs: 3),
        taskOf(id: 'b', projectId: 'q', employeeId: 'des', employeeName: 'Di', title: 'Draw it', goal: '', status: TaskStatus.running, updatedAtMs: 2),
        taskOf(id: 'c', projectId: 'q', employeeId: 'dev', employeeName: 'Cy', title: 'Ask me', goal: '', status: TaskStatus.waiting, waitingReason: 'input'),
        taskOf(id: 'd', projectId: 'p', employeeId: 'ceo', employeeName: 'Ada', title: 'Shipped', goal: '', status: TaskStatus.succeeded),
      ],
    );

    testWidgets('More lists Projects, Ideas, Company and Settings', (tester) async {
      await show(tester, state, const MorePage());
      expect(find.byType(GroupRow), findsNWidgets(4));
      expect(find.text('Activity'), findsNothing);
      await tester.tap(find.byKey(const ValueKey('more-projects')));
      await tester.pumpAndSettle();
      expect(find.text('projects page'), findsOneWidget);
      await tester.pageBack();
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('more-ideas')));
      await tester.pumpAndSettle();
      expect(find.text('ideas page'), findsOneWidget);
      await tester.pageBack();
      await tester.pumpAndSettle();
      expect(find.text('${state.activeEmployees.length} employees'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('more-company')));
      await tester.pumpAndSettle();
      expect(find.text('company page'), findsOneWidget);
      await tester.pageBack();
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('more-settings')));
      await tester.pumpAndSettle();
      expect(find.text('settings page'), findsOneWidget);
    });

    testWidgets('Tasks: every project, by state, with lead and project, filtered by employee', (tester) async {
      await show(tester, state, const TasksPage());
      expect(find.text('Build it'), findsOneWidget);
      expect(find.text('Draw it'), findsOneWidget);
      expect(find.text('Ask me'), findsNothing, reason: 'it waits for the user');
      expect(find.text('Shipped'), findsNothing);
      expect(find.textContaining('Cy · Atlas'), findsOneWidget);
      await tester.tap(find.text('Waiting 1'));
      await settle(tester);
      expect(find.text('Ask me'), findsOneWidget);
      await tester.tap(find.text('Active 2'));
      await settle(tester);
      await tester.tap(find.byKey(const ValueKey('tasks-filter-employee')));
      await settle(tester);
      await tester.tap(find.text('Di').last);
      await settle(tester);
      expect(find.text('Build it'), findsNothing);
      expect(find.text('Draw it'), findsOneWidget);
    });

    testWidgets('Tasks: a department shows its members\' tasks', (tester) async {
      await show(tester, state, const TasksPage());
      await tester.tap(find.byKey(const ValueKey('tasks-filter-department')));
      await settle(tester);
      await tester.tap(find.text('Engineering').last);
      await settle(tester);
      expect(find.text('Build it'), findsOneWidget);
      expect(find.text('Draw it'), findsNothing);
    });
  });
}
