import 'package:aurelius_mobile/app/mobile_core.dart';
import 'package:aurelius_mobile/app/router.dart';
import 'package:aurelius_mobile/app/shell.dart';
import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/chat_view.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/company_org/employee_page.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/power/power_bloc.dart';
import 'package:aurelius_mobile/features/project/task_page.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:aurelius_mobile/features/team/mentions.dart';
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

class MockRuntimeBloc extends MockBloc<RuntimeEvent, RuntimeState> implements RuntimeBloc {}

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
    when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.appends).thenAnswer((_) => const Stream.empty());
  });

  group('the Team chat', () {
    // Orion answers the group in its own chat 'own-orion', in run r-orion.
    final orion = employeeOf(id: 'o', name: 'Orion', role: 'Project Manager', conversationId: 'own-orion', agents: const [AgentView(kind: 'conversation', name: 'Conversation', detail: '', sinceMs: 1, steps: 0, runId: 'r-orion', conversationId: 'own-orion')]);
    final atlas = employeeOf(id: 'a', name: 'Atlas', conversationId: 'own-atlas');

    Future<void> pump(WidgetTester tester, List<MessageView> messages, {List<ApprovalView> approvals = const []}) async {
      when(() => repo.groupChat()).thenAnswer((_) async => snapshotOf('team', messages: messages));
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => loadOf());
      when(() => repo.powerChanges).thenAnswer((_) => const Stream.empty());
      final company = MockCompanyBloc();
      whenListen(company, const Stream<CompanyState>.empty(), initialState: CompanyState(loading: false, employees: [orion, atlas]));
      final inbox = MockInboxBloc();
      whenListen(inbox, const Stream<InboxState>.empty(), initialState: InboxState(loading: false, approvals: approvals));
      final connection = MockConnectionBloc();
      whenListen(connection, const Stream<LinkState>.empty(), initialState: online);
      final runtime = MockRuntimeBloc();
      whenListen(runtime, const Stream<RuntimeState>.empty(), initialState: const RuntimeState());
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: MultiBlocProvider(
          providers: [
            BlocProvider<CompanyBloc>.value(value: company),
            BlocProvider<InboxBloc>.value(value: inbox),
            BlocProvider<ConnectionBloc>.value(value: connection),
            BlocProvider<RuntimeBloc>.value(value: runtime),
          ],
          child: MaterialApp(theme: aureliusTheme(), home: const TeamPage()),
        ),
      ));
      await tester.pump(const Duration(milliseconds: 200));
    }

    final streaming = messageOf('live', MessageRole.assistant, 'Checking the plan', status: MessageStatus.partial, runId: 'r-orion', author: 'Orion', update: 'group');

    testWidgets("an answer streaming in shows what its run waits for: the approval carries the employee's own chat and the run", (tester) async {
      await pump(tester, [messageOf('u1', MessageRole.user, '@Orion ship it'), streaming], approvals: [
        approvalOf('a1', conversationId: 'own-orion', runId: 'r-orion'),
        approvalOf('other', conversationId: 'own-orion', runId: 'r-old'),
      ]);
      expect(find.text('Waiting for your approval: ${approvalOf('a1').title}'), findsOneWidget);
      expect(find.byKey(const ValueKey('group-ask-a1')), findsNothing, reason: 'shown under its answer');
      expect(tester.takeException(), isNull);
    });

    testWidgets("what a run in an employee's own chat asks shows in Team when no answer streams for it", (tester) async {
      await pump(tester, [messageOf('u1', MessageRole.user, '@Atlas plan it')], approvals: [
        approvalOf('a2', conversationId: 'own-atlas', runId: 'r-atlas', requester: 'Atlas'),
        approvalOf('task', conversationId: 'c-task', runId: 'r-task'),
      ]);
      expect(find.byKey(const ValueKey('group-ask-a2')), findsOneWidget);
      expect(find.text('WAITING FOR YOU · Atlas'), findsOneWidget);
      expect(find.byKey(const ValueKey('group-ask-task')), findsNothing, reason: "a task's approval belongs to the task");
    });

    testWidgets('several may answer at once: the composer has no Stop of its own, each answer has one', (tester) async {
      await pump(tester, [messageOf('u1', MessageRole.user, '@Orion @Atlas status?'), streaming]);
      expect(find.byKey(const Key('composer-stop')), findsNothing);
      expect(find.byKey(const Key('composer-send')), findsOneWidget);
      expect(find.byKey(const Key('stop-run')), findsOneWidget);
    });

    testWidgets('each sender shows with their role, and so does each @ suggestion; a tap completes the name', (tester) async {
      await pump(tester, [
        messageOf('u1', MessageRole.user, 'Status?'),
        messageOf('a1', MessageRole.assistant, 'On track.', update: 'group', author: 'Orion'),
        messageOf('a2', MessageRole.assistant, 'Agreed.', update: 'group', author: 'Atlas'),
      ]);
      expect(find.textContaining('Orion — Project Manager'), findsOneWidget);
      expect(find.textContaining('Atlas — Writer'), findsOneWidget);
      expect(find.byKey(const Key('mentions')), findsNothing, reason: 'no @ typed');

      await tester.enterText(find.byType(TextField).last, 'Hi @o');
      await tester.pump();
      expect(find.text('@Orion — Project Manager'), findsOneWidget);
      expect(find.byKey(const Key('mention-Atlas')), findsNothing);
      await tester.enterText(find.byType(TextField).last, 'Hi @');
      await tester.pump();
      expect(find.text('@Atlas — Writer'), findsOneWidget, reason: 'a bare @ offers everyone');
      await tester.tap(find.byKey(const Key('mention-Orion')));
      await tester.pump();
      expect(tester.widget<TextField>(find.byType(TextField).last).controller!.text, 'Hi @Orion ');
      expect(find.byKey(const Key('mentions')), findsNothing, reason: 'the mention is complete');
      expect(tester.takeException(), isNull);
    });

    testWidgets("a note of Aurelius (nobody was woken) is a quiet line, not somebody's answer", (tester) async {
      await pump(tester, [
        messageOf('u1', MessageRole.user, '@Atlas hi'),
        messageOf('n1', MessageRole.assistant, 'Nobody was woken: Atlas is paused. Resume them, or name a colleague with @Name.', update: 'status', author: 'Aurelius'),
        messageOf('n2', MessageRole.assistant, '(Could not answer: cancelled.)', update: 'group', author: 'Orion'),
        messageOf('n3', MessageRole.assistant, 'A note without a name', update: 'group', author: ' '),
      ]);
      expect(find.byKey(const Key('system-line')), findsNWidgets(2));
      expect(find.text('Nobody was woken: Atlas is paused. Resume them, or name a colleague with @Name.'), findsOneWidget);
      expect(find.textContaining('UPDATE'), findsNothing);
      expect(find.textContaining('Orion'), findsOneWidget, reason: "a cancelled answer stays the employee's");
      expect(tester.takeException(), isNull);
    });
  });

  test('@ suggestions follow the desktop: case-insensitive prefix, no e-mail, no name typed whole', () {
    const names = ['Atlas', 'Titan', 'Trace', 'Orion'];
    final found = mentionSuggestions('Hi @t', names)!;
    expect(found.$1, 3);
    expect(found.$2, ['Titan', 'Trace']);
    expect(mentionSuggestions('@', names)?.$2.length, 4);
    expect(mentionSuggestions('@Atlas', names), isNull);
    expect(mentionSuggestions('@Atlas please', names), isNull);
    expect(mentionSuggestions('mail me at me@or', names), isNull);
    expect(mentionSuggestions('@x', names), isNull);
    expect(completeMention('Hi @t', 3, 'Titan'), 'Hi @Titan ');
    expect(withRole('Orion', 'Project Manager'), 'Orion — Project Manager');
    expect(withRole('Orion', ' '), 'Orion');
  });

  test('a note the server writes is told apart from an answer, as on the desktop', () {
    MessageView m({String? update = 'group', String? author, MessageStatus status = MessageStatus.complete}) => messageOf('x', MessageRole.assistant, 'text', update: update, author: author, status: status);
    expect(isSystemLine(m(), inGroup: true), isTrue, reason: 'a finished group post without an author');
    expect(isSystemLine(m(author: '  '), inGroup: true), isTrue, reason: 'a blank author is none');
    expect(isSystemLine(m(update: 'system', author: 'Aurelius'), inGroup: true), isTrue);
    expect(isSystemLine(m(update: 'status', author: 'Aurelius'), inGroup: true), isTrue);
    expect(isSystemLine(m(update: 'status', author: 'Aurelius'), inGroup: false), isFalse, reason: 'in a task a status stays an update card');
    expect(isSystemLine(m(status: MessageStatus.partial), inGroup: true), isFalse, reason: 'streaming with its employee not known yet');
    expect(isSystemLine(m(author: 'Atlas'), inGroup: true), isFalse);
    expect(isSystemLine(m(author: 'Atlas', status: MessageStatus.interrupted), inGroup: true), isFalse, reason: 'cut off, it stays an answer');
    expect(isSystemLine(m(update: null), inGroup: true), isFalse);
    expect(isSystemLine(messageOf('u', MessageRole.user, 'hi', update: 'system'), inGroup: true), isFalse);
  });

  testWidgets("an employee's own chat typed in as a conversation opens Team", (tester) async {
    final company = MockCompanyBloc();
    whenListen(company, const Stream<CompanyState>.empty(), initialState: CompanyState(loading: false, employees: [employeeOf(id: 'mgr', name: 'Iris', conversationId: 'own')]));
    final connection = MockConnectionBloc();
    whenListen(connection, const Stream<LinkState>.empty(), initialState: online);
    final router = GoRouter(initialLocation: '/conversation/own', routes: [
      GoRoute(path: '/team', builder: (_, _) => const Scaffold(body: Text('page team'))),
      GoRoute(path: '/conversation/:cid', builder: (_, s) => ConversationPage(conversationId: s.pathParameters['cid']!)),
    ]);
    addTearDown(router.dispose);
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: MultiBlocProvider(
        providers: [BlocProvider<CompanyBloc>.value(value: company), BlocProvider<ConnectionBloc>.value(value: connection)],
        child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router),
      ),
    ));
    await tester.pumpAndSettle();
    expect(find.text('page team'), findsOneWidget);
    verifyNever(() => repo.messages(any()));
  });

  group('routes', () {
    late AppRouter router;

    setUp(() => router = AppRouter(host));
    tearDown(() => router.dispose());

    test('Team chat is the first tab and the Board the second; hidden screens keep their routes (ticket #19)', () {
      final shell = router.config.configuration.routes.whereType<StatefulShellRoute>().single;
      final first = [for (final b in shell.branches) (b.routes.first as GoRoute).path];
      expect(first, ['/team', '/board', '/inbox', '/home', '/more']);
      expect(first, shellTabPatterns);
      expect(router.config.routeInformationProvider.value.uri.path, '/team', reason: 'the app opens on the Team chat');
      final paths = pathsOf(router.config.configuration.routes);
      expect(paths, containsAll(['/tasks', '/board/new', '/ticket/:tid', '/ticket/:tid/plan', '/settings/board']));
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
      final router = GoRouter(initialLocation: '/team', routes: [
        StatefulShellRoute.indexedStack(
          builder: (context, state, shell) => HomeShell(shell: shell),
          branches: [
            for (final name in ['team', 'board', 'inbox', 'home', 'more'])
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

    testWidgets('the shell opens on the group chat; ⌘3 opens the inbox, ⌘1 the chat again; the Board is a tab', (tester) async {
      await pump(tester, const InboxState(loading: false));
      expect(find.text('page team'), findsOneWidget);
      await tester.tap(find.text('Home'));
      await tester.pumpAndSettle();
      expect(find.text('page home'), findsOneWidget);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.digit3);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
      await tester.pumpAndSettle();
      expect(find.text('page inbox'), findsOneWidget);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.digit1);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();
      expect(find.text('page team'), findsOneWidget);
      await tester.tap(find.text('Board'));
      await tester.pumpAndSettle();
      expect(find.text('page board'), findsOneWidget);
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

  group("a ticket's main task (task.assign/cancel are refused: use the board)", () {
    Future<void> openTask(WidgetTester tester, TaskView task) async {
      when(() => repo.projects()).thenAnswer((_) async => [projectOf(id: 'p', name: 'Atlas')]);
      when(() => repo.tasks(projectId: 'p')).thenAnswer((_) async => [task]);
      final company = MockCompanyBloc();
      whenListen(company, const Stream<CompanyState>.empty(), initialState: CompanyState(loading: false, employees: [employeeOf(id: 'mgr', name: 'Iris')]));
      final connection = MockConnectionBloc();
      whenListen(connection, const Stream<LinkState>.empty(), initialState: online);
      final router = GoRouter(initialLocation: '/task', routes: [
        GoRoute(path: '/task', builder: (_, _) => TaskPage(projectId: 'p', taskId: task.id)),
        GoRoute(path: '/ticket/:tid', builder: (_, s) => Scaffold(body: Text('ticket ${s.pathParameters['tid']}'))),
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
    }

    testWidgets('no Cancel task or Give to…, but a way to the ticket on the board', (tester) async {
      await openTask(tester, taskOf(id: 't', projectId: 'p', title: 'Ship it', employeeId: 'mgr', status: TaskStatus.queued, ticketId: 'k1', ticketNumber: 7));
      expect(find.text('Cancel task'), findsNothing);
      expect(find.text('Give to someone else'), findsNothing);
      final open = find.byKey(const Key('task-open-ticket'));
      expect(find.text('Main task of ticket #7 · owner and cancel on the board'), findsOneWidget);
      await tester.ensureVisible(open);
      await tester.tap(open);
      await tester.pumpAndSettle();
      expect(find.text('ticket k1'), findsOneWidget);
    });

    testWidgets('a delegation under a ticket keeps Cancel task and Give to…', (tester) async {
      await openTask(tester, taskOf(id: 't', projectId: 'p', title: 'Ship it', employeeId: 'mgr', status: TaskStatus.queued, ticketId: 'k1', ticketNumber: 7, parentTaskId: 'root'));
      expect(find.text('Cancel task'), findsOneWidget);
      expect(find.text('Give to someone else'), findsOneWidget);
      expect(find.byKey(const Key('task-open-ticket')), findsNothing);
    });
  });
}
