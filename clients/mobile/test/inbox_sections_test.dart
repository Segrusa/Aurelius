import 'dart:async';

import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_page.dart';
import 'package:aurelius_mobile/features/inbox/inbox_sections.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockInboxBloc extends MockBloc<InboxEvent, InboxState> implements InboxBloc {}

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

NotificationView result(String id, {String severity = 'info', String? taskId, String title = 'Result ready'}) =>
    NotificationView(id: id, severity: severity, title: title, subject: 'Subject $id', createdAtMs: 0, read: true, taskId: taskId, targetType: 'task');

TaskView task(String id, TaskStatus status, {String? waitingReason, int pendingApprovals = 0, int updatedAtMs = 0}) =>
    taskOf(id: id, projectId: 'p', title: 'Task $id', status: status, waitingReason: waitingReason, pendingApprovals: pendingApprovals, updatedAtMs: updatedAtMs);

const answering = AgentView(kind: 'conversation', name: 'Iris', detail: '', sinceMs: 5, steps: 0, conversationId: 'c-1');

Future<(MockInboxBloc, StreamController<CompanyState>)> show(WidgetTester tester, InboxState inbox, CompanyState company) async {
  tester.view.physicalSize = const Size(1200, 3600);
  addTearDown(tester.view.resetPhysicalSize);
  final inboxBloc = MockInboxBloc();
  final companyBloc = MockCompanyBloc();
  final companyChanges = StreamController<CompanyState>();
  addTearDown(companyChanges.close);
  whenListen(inboxBloc, const Stream<InboxState>.empty(), initialState: inbox);
  whenListen(companyBloc, companyChanges.stream, initialState: company);
  final router = GoRouter(routes: [
    GoRoute(
      path: '/',
      builder: (_, _) => MultiBlocProvider(
        providers: [BlocProvider<InboxBloc>.value(value: inboxBloc), BlocProvider<CompanyBloc>.value(value: companyBloc)],
        child: const InboxPage(),
      ),
    ),
    GoRoute(path: '/project/:pid/task/:tid', builder: (_, s) => Scaffold(body: Text('task ${s.pathParameters['tid']}'))),
    GoRoute(path: '/employee/:mid', builder: (_, s) => Scaffold(body: Text('employee ${s.pathParameters['mid']}'))),
  ]);
  await tester.pumpWidget(MaterialApp.router(theme: aureliusTheme(), routerConfig: router));
  await tester.pump(const Duration(milliseconds: 100));
  return (inboxBloc, companyChanges);
}

Finder header(String section) => find.byKey(ValueKey('inbox-section-$section'));

double top(WidgetTester tester, Finder f) => tester.getTopLeft(f).dy;

void main() {
  setUpAll(() {
    registerFallbackValue(const InboxRefreshed());
    registerFallbackValue(const CompanyRefreshed());
  });

  final project = projectOf(id: 'p', name: 'Atlas');

  test('Sections sort approvals, waiting, running, answers and results', () {
    final s = InboxSections.of(
      InboxState(
        loading: false,
        approvals: [approvalOf('a1', taskId: 'blocked')],
        proposals: [proposalOf('pr1')],
        notifications: [result('n1'), result('n2', severity: 'warning'), result('n3', severity: 'action', title: 'Approval needed')],
      ),
      CompanyState(
        loading: false,
        projects: [project],
        employees: [employeeOf(id: 'm1', working: true, agents: const [answering, AgentView(kind: 'conversation', name: 'Iris', detail: '', sinceMs: 1, steps: 0, taskId: 'run')])],
        tasks: [
          task('blocked', TaskStatus.running, pendingApprovals: 1),
          task('input', TaskStatus.waiting, waitingReason: 'input'),
          task('escalated', TaskStatus.waiting, waitingReason: 'escalated'),
          task('run', TaskStatus.running, updatedAtMs: 2),
          task('queued', TaskStatus.queued, updatedAtMs: 1),
          task('dependency', TaskStatus.waiting, waitingReason: 'dependency'),
          task('draft', TaskStatus.draft),
          task('ok', TaskStatus.succeeded),
          task('bad', TaskStatus.failed),
          task('stop', TaskStatus.cancelled),
        ],
      ),
    );
    expect(s.approvals.map((a) => a.id), ['a1']);
    expect(s.proposals.map((p) => p.id), ['pr1']);
    // The task blocked on approval a1 is that card, not a second row.
    expect(s.waitingTasks.map((t) => t.id), unorderedEquals(['input', 'escalated']));
    expect(s.needsYou, 4);
    expect(s.runningTasks.map((t) => t.id), ['run', 'queued', 'dependency']);
    // A task's own run is its task row; only the answer outside a task counts.
    expect(s.answers.map((a) => a.agent.conversationId), ['c-1']);
    expect(s.running, 4);
    // "Action" alerts are the live items above, not results.
    expect(s.results.map((n) => n.id), ['n1', 'n2']);
    expect(s.done, 2);
  });

  testWidgets('The inbox shows Needs you, Running and Done in order, with counts', (tester) async {
    await show(
      tester,
      InboxState(loading: false, approvals: [approvalOf('a1')], notifications: [result('n1', taskId: 'ok')]),
      CompanyState(
        loading: false,
        projects: [project],
        employees: [employeeOf(id: 'm1', name: 'Iris', working: true, agents: const [answering])],
        tasks: [task('run', TaskStatus.running), task('q', TaskStatus.queued), task('ok', TaskStatus.succeeded)],
      ),
    );
    expect(find.text('Needs you  1'), findsOneWidget);
    expect(find.text('Running  3'), findsOneWidget);
    expect(find.text('Done  1'), findsOneWidget);
    expect(top(tester, header('needs-you')), lessThan(top(tester, header('running'))));
    expect(top(tester, header('running')), lessThan(top(tester, header('done'))));
    // Rows sit under their own header and carry the usual marks.
    expect(top(tester, find.byKey(const ValueKey('inbox-task-run'))), greaterThan(top(tester, header('running'))));
    expect(top(tester, find.byKey(const ValueKey('inbox-task-run'))), lessThan(top(tester, header('done'))));
    expect(find.descendant(of: find.byKey(const ValueKey('inbox-task-run')), matching: find.byKey(const ValueKey('task-mark-working'))), findsOneWidget);
    expect(find.descendant(of: find.byKey(const ValueKey('inbox-task-q')), matching: find.byKey(const ValueKey('task-mark-queued'))), findsOneWidget);
    expect(find.byKey(const ValueKey('inbox-answer-m1-c-1')), findsOneWidget);
    expect(find.descendant(of: find.byKey(const ValueKey('notification-n1')), matching: find.byKey(const ValueKey('task-mark-done'))), findsOneWidget);
    expect(top(tester, find.byKey(const ValueKey('notification-n1'))), greaterThan(top(tester, header('done'))));
  });

  testWidgets('Empty sections keep their header with a short line', (tester) async {
    await show(
      tester,
      const InboxState(loading: false),
      CompanyState(loading: false, projects: [project], tasks: [task('run', TaskStatus.running)]),
    );
    expect(find.text('Needs you  0'), findsOneWidget);
    expect(find.text('Nothing needs you.'), findsOneWidget);
    expect(find.text('Done  0'), findsOneWidget);
    expect(find.text('No results yet.'), findsOneWidget);
    expect(find.text('Nothing running.'), findsNothing);
    expect(find.text('Clear all'), findsNothing);
  });

  testWidgets('A task moves from Running to Needs you live, and opens the task', (tester) async {
    final (_, company) = await show(
      tester,
      const InboxState(loading: false),
      CompanyState(loading: false, projects: [project], tasks: [task('t', TaskStatus.running)]),
    );
    expect(find.text('Running  1'), findsOneWidget);
    expect(find.text('Needs you  0'), findsOneWidget);
    company.add(CompanyState(loading: false, projects: [project], tasks: [task('t', TaskStatus.waiting, waitingReason: 'input')]));
    await tester.pump();
    expect(find.text('Running  0'), findsOneWidget);
    expect(find.text('Needs you  1'), findsOneWidget);
    final row = find.byKey(const ValueKey('inbox-task-t'));
    expect(top(tester, row), lessThan(top(tester, header('running'))));
    expect(find.descendant(of: row, matching: find.byKey(const ValueKey('task-mark-needsYou'))), findsOneWidget);
    await tester.tap(row);
    await tester.pumpAndSettle();
    expect(find.text('task t'), findsOneWidget);
  });

  testWidgets('Clear all sits on Done, asks first and only clears results', (tester) async {
    final (bloc, _) = await show(
      tester,
      InboxState(loading: false, approvals: [approvalOf('a1')], notifications: [result('n1'), result('n2')]),
      CompanyState(loading: false, projects: [project]),
    );
    final events = <InboxEvent>[];
    when(() => bloc.add(any())).thenAnswer((i) => events.add(i.positionalArguments.first as InboxEvent));
    expect(find.descendant(of: header('done'), matching: find.text('Clear all')), findsOneWidget);
    expect(find.descendant(of: header('needs-you'), matching: find.text('Clear all')), findsNothing);
    await tester.tap(find.text('Clear all'));
    await tester.pumpAndSettle();
    expect(find.text('Clear all 2 results?'), findsOneWidget);
    expect(find.text('Approvals are not touched.'), findsOneWidget);
    await tester.tap(find.widgetWithText(TextButton, 'Clear all'));
    await tester.pumpAndSettle();
    expect(events.whereType<NotificationsCleared>(), hasLength(1));
    expect(events.whereType<ApprovalDecided>(), isEmpty);
  });
}
