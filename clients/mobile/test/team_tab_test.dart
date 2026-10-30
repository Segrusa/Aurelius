import 'package:aurelius_mobile/app/router.dart';
import 'package:aurelius_mobile/app/shell.dart';
import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/power/power_bloc.dart';
import 'package:aurelius_mobile/features/project/task_page.dart';
import 'package:aurelius_mobile/features/team/team_page.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

class MockInboxBloc extends MockBloc<InboxEvent, InboxState> implements InboxBloc {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

class MockPowerBloc extends MockBloc<PowerEvent, PowerBlocState> implements PowerBloc {}

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

const online = LinkState(status: LinkStatus.online, capabilities: ['read', 'chat']);
const host = Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0);

/// Every route's full path, the shell's branches included.
List<String> pathsOf(List<RouteBase> routes, [String parent = '']) => [
      for (final r in routes) ...switch (r) {
        GoRoute(:final path) => [
            path.startsWith('/') ? path : '$parent/$path',
            ...pathsOf(r.routes, path.startsWith('/') ? path : '$parent/$path'),
          ],
        StatefulShellRoute(:final branches) => [for (final b in branches) ...pathsOf(b.routes, parent)],
        _ => pathsOf(r.routes, parent),
      },
    ];

void main() {
  late MockRepository repo;

  setUp(() {
    repo = MockRepository();
    when(() => repo.host).thenReturn(host);
    when(() => repo.linkState).thenReturn(online);
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.changes).thenAnswer((_) => const Stream.empty());
  });

  group('routes', () {
    late AppRouter router;

    setUp(() => router = AppRouter(host));
    tearDown(() => router.dispose());

    test('Team is the second tab, right after Home', () {
      final shell = router.config.configuration.routes.whereType<StatefulShellRoute>().single;
      final first = [for (final b in shell.branches) (b.routes.first as GoRoute).path];
      expect(first, ['/home', '/team', '/inbox', '/tasks', '/more']);
      expect(first.length, shellTabs.length, reason: 'one tab per branch');
    });

    test("an employee's own chat has no route any more; conversations have their own", () {
      final paths = pathsOf(router.config.configuration.routes);
      expect(paths, contains('/employee/:eid'));
      expect(paths, contains('/employee/:eid/tasks'));
      expect(paths, isNot(contains('/employee/:eid/chat')));
      expect(paths.where((p) => p.endsWith('/chat')), isEmpty);
      expect(paths, contains('/conversation/:cid'));
    });
  });

  group('HomeShell', () {
    Future<void> pump(WidgetTester tester, InboxState inbox) async {
      final inboxBloc = MockInboxBloc();
      whenListen(inboxBloc, const Stream<InboxState>.empty(), initialState: inbox);
      final connection = MockConnectionBloc();
      whenListen(connection, const Stream<LinkState>.empty(), initialState: online);
      final power = MockPowerBloc();
      whenListen(power, const Stream<PowerBlocState>.empty(), initialState: const PowerBlocState());
      final router = GoRouter(initialLocation: '/home', routes: [
        StatefulShellRoute.indexedStack(
          builder: (context, state, shell) => HomeShell(shell: shell),
          branches: [
            for (final name in ['home', 'team', 'inbox', 'tasks', 'more'])
              StatefulShellBranch(routes: [GoRoute(path: '/$name', builder: (_, _) => Scaffold(body: Center(child: Text('page $name'))))]),
          ],
        ),
      ]);
      addTearDown(router.dispose);
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: MultiBlocProvider(
          providers: [
            BlocProvider<InboxBloc>.value(value: inboxBloc),
            BlocProvider<ConnectionBloc>.value(value: connection),
            BlocProvider<PowerBloc>.value(value: power),
          ],
          child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router),
        ),
      ));
      await tester.pump();
    }

    testWidgets('the inbox badge stays on the Inbox tab, not on Team', (tester) async {
      await pump(tester, InboxState(loading: false, approvals: [approvalOf('a1'), approvalOf('a2')]));
      final badge = find.ancestor(of: find.text('2'), matching: find.byType(Badge));
      expect(badge, findsOneWidget);
      final x = tester.getCenter(badge).dx;
      expect((x - tester.getCenter(find.text('Inbox')).dx).abs(), lessThan(1));
      expect((x - tester.getCenter(find.text('Team')).dx).abs(), greaterThan(10));
    });

    testWidgets('a tap on Team and ⌘2 open the group chat; ⌘3 the inbox', (tester) async {
      await pump(tester, const InboxState(loading: false));
      expect(find.text('page home'), findsOneWidget);
      await tester.tap(find.text('Team'));
      await tester.pumpAndSettle();
      expect(find.text('page team'), findsOneWidget);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.digit3);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
      await tester.pumpAndSettle();
      expect(find.text('page inbox'), findsOneWidget);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.digit2);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();
      expect(find.text('page team'), findsOneWidget);
    });
  });

  test("a streaming answer's author is the employee whose agent runs it", () {
    final company = CompanyState(employees: [
      employeeOf(id: 'a', name: 'Atlas'),
      employeeOf(id: 'o', name: 'Orion', agents: const [AgentView(kind: 'conversation', name: 'Orion', detail: '', sinceMs: 1, steps: 0, runId: 'r-1')]),
    ]);
    expect(runAuthorOf(company, 'r-1'), 'Orion');
    expect(runAuthorOf(company, 'r-2'), isNull);
  });

  testWidgets("the task's Open conversation opens its own route (writable, not an employee chat)", (tester) async {
    when(() => repo.projects()).thenAnswer((_) async => [projectOf(id: 'p', name: 'Atlas')]);
    when(() => repo.tasks(projectId: 'p')).thenAnswer((_) async => [taskOf(id: 't', projectId: 'p', title: 'Ship it', employeeId: 'mgr', conversationId: 'c-task')]);
    final company = MockCompanyBloc();
    whenListen(company, const Stream<CompanyState>.empty(), initialState: CompanyState(loading: false, employees: [employeeOf(id: 'mgr', name: 'Iris')]));
    final connection = MockConnectionBloc();
    whenListen(connection, const Stream<LinkState>.empty(), initialState: online);
    final router = GoRouter(initialLocation: '/task', routes: [
      GoRoute(path: '/task', builder: (_, _) => const TaskPage(projectId: 'p', taskId: 't')),
      GoRoute(path: '/conversation/:cid', builder: (_, s) => Scaffold(body: Text('conversation ${s.pathParameters['cid']}'))),
      GoRoute(path: '/employee/:eid/chat', builder: (_, s) => const Scaffold(body: Text('employee chat'))),
    ]);
    addTearDown(router.dispose);
    tester.view.physicalSize = const Size(800, 2400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: MultiBlocProvider(
        providers: [BlocProvider<CompanyBloc>.value(value: company), BlocProvider<ConnectionBloc>.value(value: connection)],
        child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router),
      ),
    ));
    await tester.pump(const Duration(milliseconds: 200));
    final open = find.widgetWithText(OutlinedButton, 'Open conversation');
    await tester.ensureVisible(open);
    await tester.tap(open);
    await tester.pumpAndSettle();
    expect(find.text('conversation c-task'), findsOneWidget);
    expect(find.text('employee chat'), findsNothing);
  });
}
