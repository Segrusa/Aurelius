import 'package:aurelius_mobile/core/net/connection.dart' show LinkState, LinkStatus;
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/widgets/widgets.dart' show StatusDot;
import 'package:aurelius_mobile/core/widgets/ios.dart' show Glass;
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/discussion.dart' show TicketDraft;
import 'package:aurelius_mobile/data/protocol/protocol.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/company_org/employee_page.dart' show ConversationPage;
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:aurelius_mobile/features/sessions/new_session_page.dart';
import 'package:aurelius_mobile/features/sessions/session_page.dart';
import 'package:aurelius_mobile/features/sessions/session_screen_rules.dart';
import 'package:aurelius_mobile/features/sessions/sessions_page.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// Ticket #71: the Sessions tab, New session and a session's screen on a phone.

LinkState linkOf(List<String> features) => LinkState(status: LinkStatus.online, capabilities: const ['read', 'chat', 'task_control'], features: features, boardMembersExist: true, user: testUser, claudeLogin: testLogin);

final sessionsLink = linkOf(const ['work_sessions', 'session_dispatch', 'chat_files']);

WorkSessionView sessionOf(String id, {String title = 'Fix the login', int at = 0, bool running = false, SessionDispatch? dispatch, String employeeId = 'forge', String employee = 'Forge', bool unseen = false}) => WorkSessionView(
      conversationId: id,
      createdAtMs: 0,
      employeeId: employeeId,
      employeeName: employee,
      lastActivityMs: at,
      projectId: 'p1',
      projectName: 'Atlas',
      running: running,
      title: title,
      dispatch: dispatch,
      unseen: unseen,
    );

MockRepository repoWith({List<WorkSessionView> sessions = const [], LinkState? link, MessagesSnapshot? messages, BoardView? board}) {
  final repo = boardRepository(link: link ?? sessionsLink, messages: messages, board: board);
  when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
  when(() => repo.appends).thenAnswer((_) => const Stream.empty());
  when(() => repo.link).thenAnswer((_) => const Stream.empty());
  when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
  when(() => repo.load()).thenAnswer((_) async => loadOf());
  when(() => repo.powerChanges).thenAnswer((_) => const Stream.empty());
  when(() => repo.boardMembers()).thenAnswer((_) async => const [testUser]);
  when(() => repo.workSessions()).thenAnswer((_) async => sessions);
  return repo;
}

/// Opens the session's Properties behind the head's "…".
Future<void> openSessionProperties(WidgetTester tester) async {
  await tester.tap(find.byKey(const ValueKey('session-more')));
  await settleBoard(tester);
}

/// [page] under a router that knows where the session pages lead.
Future<GoRouter> pumpSessions(WidgetTester tester, MockRepository repo, Widget page, {LinkState? link, CompanyState? company}) async {
  tester.view.physicalSize = const Size(390, 844) * 3;
  tester.view.devicePixelRatio = 3;
  addTearDown(tester.view.reset);
  final state = link ?? repo.linkState;
  final companyBloc = MockCompanyBloc();
  whenListen(companyBloc, const Stream<CompanyState>.empty(), initialState: company ?? companyWith());
  final connection = MockConnectionBloc();
  whenListen(connection, const Stream<LinkState>.empty(), initialState: state);
  final inbox = MockInboxBloc();
  whenListen(inbox, const Stream<InboxState>.empty(), initialState: const InboxState(loading: false));
  final runtime = MockRuntimeBloc();
  whenListen(runtime, const Stream<RuntimeState>.empty(), initialState: const RuntimeState());
  final router = GoRouter(initialLocation: '/', routes: [
    GoRoute(path: '/', builder: (_, _) => page),
    GoRoute(path: '/chat', builder: (_, _) => const Scaffold(body: Text('page chat'))),
    GoRoute(path: '/session/:cid', builder: (_, s) => Scaffold(body: Text('session ${s.pathParameters['cid']}'))),
    GoRoute(path: '/ticket/:tid', builder: (_, s) => Scaffold(body: Text('ticket ${s.pathParameters['tid']}'))),
    GoRoute(
      path: '/board/new',
      builder: (_, s) => Scaffold(body: Text('page new ticket ${s.uri.queryParameters['project'] ?? ''} ${s.extra is TicketDraft ? '${(s.extra as TicketDraft).title}|${(s.extra as TicketDraft).description}' : ''}')),
    ),
  ]);
  addTearDown(router.dispose);
  await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
    value: repo,
    child: MultiBlocProvider(
      providers: [
        BlocProvider<CompanyBloc>.value(value: companyBloc),
        BlocProvider<ConnectionBloc>.value(value: connection),
        BlocProvider<InboxBloc>.value(value: inbox),
        BlocProvider<RuntimeBloc>.value(value: runtime),
      ],
      child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router),
    ),
  ));
  await settleBoard(tester);
  return router;
}

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));
  setUp(() => SharedPreferences.setMockInitialValues({}));

  group('the Sessions tab', () {
    testWidgets('lists work_session.list newest first with who, project and the dots in front; who works is the tab badge, not a header count', (tester) async {
      final repo = repoWith(sessions: [
        sessionOf('c1', title: 'Old and quiet', at: 1),
        sessionOf('c2', title: 'Busy one', at: 3, running: true),
        sessionOf('c3', title: 'Nobody chosen', at: 2, dispatch: SessionDispatch.nobody, employee: 'Orion', employeeId: 'orion'),
      ]);
      await pumpSessions(tester, repo, const WorkSessionsPage());
      expect(find.text('Sessions'), findsWidgets);
      expect(find.text('1 working'), findsNothing);
      expect(find.text('ACTIVE · 2'), findsOneWidget, reason: 'the working one and the one waiting for your choice');
      final busy = tester.getTopLeft(find.text('Busy one')).dy;
      final nobody = tester.getTopLeft(find.text('Nobody chosen')).dy;
      final old = tester.getTopLeft(find.text('Old and quiet')).dy;
      expect(busy < nobody && nobody < old, isTrue, reason: 'newest activity first');
      expect(find.descendant(of: find.byKey(const ValueKey('session-c2')), matching: find.byKey(const ValueKey('session-working'))), findsOneWidget);
      expect(find.descendant(of: find.byKey(const ValueKey('session-c3')), matching: find.byKey(const ValueKey('session-needs-you'))), findsOneWidget);
      expect(find.descendant(of: find.byKey(const ValueKey('session-c1')), matching: find.byType(StatusDot)), findsNothing);
      expect(tester.getTopLeft(find.byKey(const ValueKey('session-working'))).dx, lessThan(tester.getTopLeft(find.text('Busy one')).dx), reason: 'the dot leads the row');
      expect(find.textContaining('Forge · Atlas'), findsNWidgets(2));
      expect(find.textContaining('Nobody yet · Atlas'), findsOneWidget);
    });

    testWidgets('a finished answer not opened yet gets the orange done dot and stays out of Active', (tester) async {
      final repo = repoWith(sessions: [sessionOf('c1', title: 'Finished one', at: 1, unseen: true)]);
      await pumpSessions(tester, repo, const WorkSessionsPage());
      expect(find.descendant(of: find.byKey(const ValueKey('session-c1')), matching: find.byKey(const ValueKey('session-done'))), findsOneWidget);
      expect(find.textContaining('ACTIVE'), findsNothing);
    });

    testWidgets('sessions stand in sections: Active, then by the day of their last activity', (tester) async {
      final now = DateTime.now();
      final repo = repoWith(sessions: [
        sessionOf('c1', title: 'From long ago', at: 1),
        sessionOf('c2', title: 'Busy one', at: 1, running: true),
        sessionOf('c3', title: 'Earlier today', at: DateTime(now.year, now.month, now.day).millisecondsSinceEpoch),
      ]);
      await pumpSessions(tester, repo, const WorkSessionsPage());
      final active = tester.getTopLeft(find.byKey(const ValueKey('sessions-section-active'))).dy;
      final today = tester.getTopLeft(find.byKey(const ValueKey('sessions-section-today'))).dy;
      final older = tester.getTopLeft(find.byKey(const ValueKey('sessions-section-older'))).dy;
      expect(active < tester.getTopLeft(find.text('Busy one')).dy, isTrue);
      expect(tester.getTopLeft(find.text('Busy one')).dy < today && today < tester.getTopLeft(find.text('Earlier today')).dy, isTrue);
      expect(tester.getTopLeft(find.text('Earlier today')).dy < older && older < tester.getTopLeft(find.text('From long ago')).dy, isTrue);
      expect(find.byKey(const ValueKey('sessions-section-yesterday')), findsNothing, reason: 'an empty section is left out');
    });

    testWidgets('a tap opens the session', (tester) async {
      final repo = repoWith(sessions: [sessionOf('c1')]);
      await pumpSessions(tester, repo, const WorkSessionsPage());
      await tester.tap(find.text('Fix the login'));
      await settleBoard(tester);
      expect(find.text('session c1'), findsOneWidget);
    });

    testWidgets('a server without work sessions says so, reads nothing and offers New ticket', (tester) async {
      final repo = repoWith(link: onlineLink);
      await pumpSessions(tester, repo, const WorkSessionsPage());
      expect(find.byKey(const ValueKey('sessions-unsupported')), findsOneWidget);
      expect(find.text('Could not load your sessions.'), findsNothing);
      verifyNever(() => repo.workSessions());
      await tester.tap(find.text('New ticket'));
      await settleBoard(tester);
      expect(find.textContaining('page new ticket'), findsOneWidget);
    });

    testWidgets('a failed read shows what failed and Try again, never an endless loading', (tester) async {
      final repo = repoWith();
      when(() => repo.workSessions()).thenThrow(const ProtocolException(ErrorCode.internal, 'The server broke.'));
      await pumpSessions(tester, repo, const WorkSessionsPage());
      expect(find.text('Could not load your sessions.'), findsOneWidget);
      expect(find.byType(CircularProgressIndicator), findsNothing);
      when(() => repo.workSessions()).thenAnswer((_) async => [sessionOf('c1')]);
      await tester.tap(find.byKey(const ValueKey('sessions-retry')));
      await settleBoard(tester);
      expect(find.text('Fix the login'), findsOneWidget);
    });

    testWidgets('the page has a large title and no ✎: work starts on the Chat tab', (tester) async {
      final repo = repoWith(sessions: [sessionOf('c1')]);
      await pumpSessions(tester, repo, const WorkSessionsPage());
      expect(find.text('Sessions'), findsWidgets);
      expect(find.byKey(const ValueKey('sessions-new')), findsNothing);
    });

    testWidgets('an empty list still has the switch and offers New session, which opens Chat', (tester) async {
      final repo = repoWith();
      await pumpSessions(tester, repo, const WorkSessionsPage());
      expect(find.byKey(const ValueKey('sessions-switch')), findsOneWidget);
      await tester.tap(find.text('New session'));
      await settleBoard(tester);
      expect(find.text('page chat'), findsOneWidget);
    });

    testWidgets('Tickets lists the open tickets: what needs you or works first, no Backlog, Done or archived', (tester) async {
      final now = DateTime.now();
      final today = DateTime(now.year, now.month, now.day, 0, 1).millisecondsSinceEpoch;
      final repo = repoWith(board: boardOf([
        ticketOf(id: 'k1', number: 1, title: 'Quiet in Todo', column: ColumnKind.todo, ownerEmployeeId: 'forge', ownerName: 'Forge', updatedAtMs: today),
        ticketOf(id: 'k2', number: 2, title: 'Report waits', column: ColumnKind.review, updatedAtMs: 1),
        ticketOf(id: 'k3', number: 3, title: 'Still in Backlog', column: ColumnKind.backlog, updatedAtMs: today),
        ticketOf(id: 'k4', number: 4, title: 'All done', column: ColumnKind.done, updatedAtMs: today),
        ticketOf(id: 'k5', number: 5, title: 'Archived one', column: ColumnKind.doing, archivedAtMs: 1, updatedAtMs: today),
      ]));
      await pumpSessions(tester, repo, const WorkSessionsPage(tickets: true));
      expect(find.text('#2 Report waits'), findsOneWidget);
      expect(find.text('#1 Quiet in Todo'), findsOneWidget);
      expect(find.textContaining('Backlog'), findsNothing);
      expect(find.textContaining('All done'), findsNothing);
      expect(find.textContaining('Archived one'), findsNothing);
      expect(find.text('ACTIVE · 1'), findsOneWidget);
      final active = tester.getTopLeft(find.byKey(const ValueKey('tickets-section-active'))).dy;
      final todaySection = tester.getTopLeft(find.byKey(const ValueKey('tickets-section-today'))).dy;
      expect(active < tester.getTopLeft(find.text('#2 Report waits')).dy && tester.getTopLeft(find.text('#2 Report waits')).dy < todaySection, isTrue);
      expect(find.descendant(of: find.byKey(const ValueKey('ticket-row-k2')), matching: find.byKey(const ValueKey('session-needs-you'))), findsOneWidget);
      await tester.tap(find.text('#1 Quiet in Todo'));
      await settleBoard(tester);
      expect(find.text('ticket k1'), findsOneWidget);
    });

    testWidgets('the switch moves between Sessions and Tickets', (tester) async {
      final repo = repoWith(sessions: [sessionOf('c1')], board: boardOf([ticketOf(id: 'k1', number: 7, title: 'Open one', column: ColumnKind.doing)]));
      await pumpSessions(tester, repo, const WorkSessionsPage());
      expect(find.text('Fix the login'), findsOneWidget);
      expect(find.text('#7 Open one'), findsNothing);
      await tester.tap(find.descendant(of: find.byKey(const ValueKey('sessions-switch')), matching: find.text('Tickets')));
      await settleBoard(tester);
      expect(find.text('#7 Open one'), findsOneWidget);
      expect(find.text('Fix the login'), findsNothing);
    });

    testWidgets('opening a session does not move it up: the list follows the activity alone', (tester) async {
      final now = DateTime.now();
      final early = DateTime(now.year, now.month, now.day, 0, 1).millisecondsSinceEpoch;
      // An open time from the older build that sorted on it: it no longer counts.
      SharedPreferences.setMockInitialValues({'sessions.used:h': '{"session:old": ${early + 1000}}'});
      final repo = repoWith(sessions: [sessionOf('new', title: 'Newer activity', at: early + 500), sessionOf('old', title: 'Opened here', at: 1)]);
      await pumpSessions(tester, repo, const WorkSessionsPage());
      expect(find.byKey(const ValueKey('sessions-section-older')), findsOneWidget);
      expect(tester.getTopLeft(find.text('Newer activity')).dy < tester.getTopLeft(find.text('Opened here')).dy, isTrue);
    });
  });

  group('New session', () {
    Future<void> choose(WidgetTester tester, String row, String option) async {
      await tester.ensureVisible(find.byKey(ValueKey('new-session-$row')));
      await tester.tap(find.byKey(ValueKey('new-session-$row')));
      await settleBoard(tester);
      await tester.tap(find.byKey(ValueKey(option)));
      await settleBoard(tester);
    }

    /// Opens the run pill's sheet, taps each option in it and closes it with Done.
    Future<void> setRun(WidgetTester tester, List<String> options) async {
      await tester.tap(find.byKey(const ValueKey('new-session-run')));
      await settleBoard(tester);
      for (final option in options) {
        await tester.ensureVisible(find.byKey(ValueKey(option)));
        await tester.tap(find.byKey(ValueKey(option)));
        await settleBoard(tester);
      }
      await tester.ensureVisible(find.byKey(const ValueKey('run-sheet-done')));
      await tester.tap(find.byKey(const ValueKey('run-sheet-done')));
      await settleBoard(tester);
    }

    String runLabel(WidgetTester tester) => tester.widget<Text>(find.descendant(of: find.byKey(const ValueKey('new-session-run')), matching: find.byType(Text))).data!;

    testWidgets('the project is a picker: its menu lists every open project with a check on the chosen one', (tester) async {
      final repo = repoWith();
      ProjectSummary project(String id, String name) => ProjectSummary(createdAtMs: 0, id: id, name: name, status: ProjectStatus.active, version: 1, employeesActive: 0);
      await pumpSessions(tester, repo, const NewSessionPage(), company: companyWith(projects: [project('p1', 'Atlas'), project('p2', 'Planora'), project('p3', 'Eurocontrol MMIT')]));
      expect(find.descendant(of: find.byKey(const ValueKey('chat-project')), matching: find.text('Choose a project')), findsOneWidget, reason: 'several projects and none used yet');
      await tester.tap(find.byKey(const ValueKey('chat-project')));
      await settleBoard(tester);
      for (final id in ['p1', 'p2', 'p3']) {
        expect(find.byKey(ValueKey('menu-$id')), findsOneWidget);
      }
      await tester.tap(find.byKey(const ValueKey('menu-p2')));
      await settleBoard(tester);
      expect(find.descendant(of: find.byKey(const ValueKey('chat-project')), matching: find.text('Planora')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('chat-project')));
      await settleBoard(tester);
      expect(find.descendant(of: find.byKey(const ValueKey('menu-p2')), matching: find.byIcon(CupertinoIcons.checkmark_alt)), findsOneWidget);
      expect(find.descendant(of: find.byKey(const ValueKey('menu-p1')), matching: find.byIcon(CupertinoIcons.checkmark_alt)), findsNothing);
    });

    testWidgets('Who = Dispatcher starts with work_session.dispatch and opens the new session', (tester) async {
      final repo = repoWith();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultWorkSessionDispatching(conversationId: 'c9', messageId: 'm', projectId: 'p1')));
      await pumpSessions(tester, repo, const NewSessionPage());
      expect(find.text('What should happen next?'), findsOneWidget);
      expect(find.text('Atlas'), findsOneWidget, reason: 'the only open project is chosen');
      expect(find.descendant(of: find.byKey(const ValueKey('chat-project')), matching: find.text('Atlas')), findsOneWidget);
      expect(find.text('Dispatcher'), findsOneWidget, reason: 'with session_dispatch the dispatcher chooses unless told');
      await tester.enterText(find.byKey(const ValueKey('new-session-text')), 'Fix the login page');
      await setRun(tester, ['pick-Model-default', 'pick-Effort-high', 'pick-Mode-plan']);
      expect(runLabel(tester), 'High · Plan', reason: 'the pill sums up what differs from the employee');
      expect(find.textContaining('The dispatcher chooses who takes it'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('new-session-start')));
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'work_session.dispatch');
      expect(paramsOf(write)['project_id'], 'p1');
      expect(paramsOf(write)['text'], 'Fix the login page');
      expect(paramsOf(write)['run'], {'effort': 'high', 'mode': 'plan'});
      expect(paramsOf(write).containsKey('employee_id'), isFalse);
      expect(find.text('session c9'), findsOneWidget);
    });

    testWidgets('Who = an employee starts with work_session.start and remembers the choice for this server', (tester) async {
      final repo = repoWith();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultWorkSessionStarted(conversationId: 'c8', messageId: 'm', projectId: 'p1', runId: 'r')));
      await pumpSessions(tester, repo, const NewSessionPage());
      await tester.enterText(find.byKey(const ValueKey('new-session-text')), 'Look at the export');
      await choose(tester, 'who', 'pick-Who-forge');
      expect(find.textContaining("Forge works in the project's folder itself"), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('new-session-start')));
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'work_session.start');
      expect(paramsOf(write)['employee_id'], 'forge');
      expect(paramsOf(write).containsKey('run'), isFalse, reason: "all the employee's defaults");
      expect(find.text('session c8'), findsOneWidget);
      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getString('new_session.who:h'), 'forge');
      expect(prefs.getString('new_session.project:h'), 'p1');
    });

    testWidgets('the model, effort and mode of a start are remembered for this server', (tester) async {
      final repo = repoWith();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultWorkSessionDispatching(conversationId: 'c9', messageId: 'm', projectId: 'p1')));
      await pumpSessions(tester, repo, const NewSessionPage());
      await tester.enterText(find.byKey(const ValueKey('new-session-text')), 'First');
      await setRun(tester, ['pick-Effort-high', 'pick-Mode-plan']);
      await tester.tap(find.byKey(const ValueKey('new-session-start')));
      await settleBoard(tester);
      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getString('new_session.effort:h'), 'high');
      expect(prefs.getString('new_session.mode:h'), 'plan');
      expect(prefs.getString('new_session.model:h'), 'default');
    });

    testWidgets('a new session opens on the remembered model, effort and mode and starts with them', (tester) async {
      SharedPreferences.setMockInitialValues({'new_session.model:h': 'claude-opus-5-5', 'new_session.effort:h': 'high', 'new_session.mode:h': 'plan'});
      final repo = repoWith();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultWorkSessionDispatching(conversationId: 'c10', messageId: 'm', projectId: 'p1')));
      await pumpSessions(tester, repo, const NewSessionPage());
      expect(runLabel(tester), endsWith('High · Plan'));
      await tester.enterText(find.byKey(const ValueKey('new-session-text')), 'Second');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('new-session-start')));
      await settleBoard(tester);
      // The runtime's model list has not loaded in this test, so the remembered model is kept.
      expect(paramsOf(writesOf(repo).single)['run'], {'model': 'claude-opus-5-5', 'effort': 'high', 'mode': 'plan'});
    });

    testWidgets('a remembered effort or mode the app no longer offers falls back to the default', (tester) async {
      SharedPreferences.setMockInitialValues({'new_session.effort:h': 'extreme', 'new_session.mode:h': 'yolo'});
      await pumpSessions(tester, repoWith(), const NewSessionPage());
      expect(runLabel(tester), 'Default');
    });

    testWidgets('without session_dispatch there is no Dispatcher: Who must be chosen', (tester) async {
      final link = linkOf(const ['work_sessions']);
      final repo = repoWith(link: link);
      await pumpSessions(tester, repo, const NewSessionPage());
      expect(find.text('Dispatcher'), findsNothing);
      await tester.enterText(find.byKey(const ValueKey('new-session-text')), 'Something');
      await tester.tap(find.byKey(const ValueKey('new-session-start')));
      await settleBoard(tester);
      expect(find.text('Choose who does it.'), findsOneWidget);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
      await tester.tap(find.byKey(const ValueKey('new-session-who')));
      await settleBoard(tester);
      expect(find.byKey(const ValueKey('pick-Who-dispatcher')), findsNothing);
    });

    testWidgets('a server without work sessions only makes tickets: the text goes to New ticket as a draft', (tester) async {
      final repo = repoWith(link: onlineLink);
      await pumpSessions(tester, repo, const NewSessionPage());
      expect(find.byKey(const ValueKey('chat-kind')), findsNothing);
      expect(find.byKey(const ValueKey('new-session-who')), findsNothing);
      await tester.enterText(find.byKey(const ValueKey('new-session-text')), 'Fix the footer\nIt overlaps on phones');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('new-session-start')));
      await settleBoard(tester);
      expect(find.text('page new ticket p1 Fix the footer|It overlaps on phones'), findsOneWidget);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
    });

    testWidgets('Ticket hands the text to New ticket; the choice is remembered for this server', (tester) async {
      final repo = repoWith();
      await pumpSessions(tester, repo, const NewSessionPage());
      await tester.tap(find.descendant(of: find.byKey(const ValueKey('chat-kind')), matching: find.text('Ticket')));
      await settleBoard(tester);
      expect(find.byKey(const ValueKey('new-session-who')), findsNothing);
      await tester.enterText(find.byKey(const ValueKey('new-session-text')), 'Write the release notes');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('new-session-start')));
      await settleBoard(tester);
      expect(find.text('page new ticket p1 Write the release notes|'), findsOneWidget);
      final prefs = await SharedPreferences.getInstance();
      expect(prefs.getString('new_session.kind:h'), 'ticket');
    });

    testWidgets('Continue offers your tickets in Doing, Testing and Review, newest first', (tester) async {
      final repo = repoWith(link: linkOf(const ['work_sessions', 'my_tickets']));
      when(() => repo.myTickets(projectId: any(named: 'projectId'), includeDone: any(named: 'includeDone'))).thenAnswer((_) async => MyTicketsView(following: const [], tickets: [
            ticketOf(id: 'k1', number: 1, title: 'In Todo', column: ColumnKind.todo, updatedAtMs: 9),
            ticketOf(id: 'k2', number: 2, title: 'Doing it', column: ColumnKind.doing, updatedAtMs: 2),
            ticketOf(id: 'k3', number: 3, title: 'To review', column: ColumnKind.review, updatedAtMs: 5),
          ]));
      await pumpSessions(tester, repo, const NewSessionPage());
      expect(find.text('#1 In Todo'), findsNothing);
      expect(tester.getTopLeft(find.text('#3 To review')).dy < tester.getTopLeft(find.text('#2 Doing it')).dy, isTrue);
      await tester.tap(find.text('#2 Doing it'));
      await settleBoard(tester);
      expect(find.text('ticket k2'), findsOneWidget);
    });

    testWidgets('after a start the composer is empty again for the next one', (tester) async {
      final repo = repoWith();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultWorkSessionDispatching(conversationId: 'c9', messageId: 'm', projectId: 'p1')));
      final router = await pumpSessions(tester, repo, const NewSessionPage());
      await tester.enterText(find.byKey(const ValueKey('new-session-text')), 'Fix the login page');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('new-session-start')));
      await settleBoard(tester);
      expect(find.text('session c9'), findsOneWidget);
      router.pop();
      await settleBoard(tester);
      expect(tester.widget<TextField>(find.byKey(const ValueKey('new-session-text'))).controller!.text, isEmpty);
    });
  });

  group('a session', () {
    testWidgets('shows its title and the Session pill, not who works where; a message goes in with chat.send', (tester) async {
      final repo = repoWith(sessions: [sessionOf('c1')], messages: snapshotOf('c1', messages: [messageOf('m1', MessageRole.user, 'Fix the login')]));
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultChatMessageSent(conversationId: 'c1', messageId: 'm2', runId: 'r2')));
      await pumpSessions(tester, repo, const SessionPage(conversationId: 'c1'));
      expect(find.text('Fix the login'), findsWidgets);
      expect(find.byKey(const ValueKey('session-kind-session')), findsOneWidget);
      expect(find.textContaining('folder'), findsNothing, reason: 'where it works is in Properties');
      expect(find.text('Reply to Forge'), findsOneWidget);
      await tester.enterText(find.byType(TextField).last, 'Also the logout');
      await tester.pump();
      await tester.tap(find.byKey(const Key('composer-send')));
      await settleBoard(tester);
      final send = writesOf(repo).single;
      expect(send['method'], 'chat.send');
      expect(paramsOf(send)['conversation_id'], 'c1');
      expect(paramsOf(send)['text'], 'Also the logout');
    });

    testWidgets('the composer input is exactly as high as its buttons', (tester) async {
      final repo = repoWith(sessions: [sessionOf('c1')], messages: snapshotOf('c1', messages: [messageOf('m1', MessageRole.user, 'Fix the login')]));
      await pumpSessions(tester, repo, const SessionPage(conversationId: 'c1'));
      final pill = find.byKey(const Key('composer-pill'));
      final input = tester.getSize(find.descendant(of: pill, matching: find.byType(TextField))).height;
      final attach = tester.getSize(find.ancestor(of: find.byKey(const Key('composer-plus')), matching: find.byType(SizedBox)).first).height;
      final send = tester.getSize(find.descendant(of: find.byKey(const Key('composer-send')), matching: find.byType(Material)).first).height;
      expect(attach, 40);
      expect(send, 40);
      expect(input, 40);
    });

    testWidgets('the pill hugs the clip: the same gap left, above and below, and no glass shadow inside', (tester) async {
      final repo = repoWith(sessions: [sessionOf('c1')], messages: snapshotOf('c1', messages: [messageOf('m1', MessageRole.user, 'Fix the login')]));
      await pumpSessions(tester, repo, const SessionPage(conversationId: 'c1'));
      final pill = tester.getRect(find.byKey(const Key('composer-pill')));
      final clip = tester.getRect(find.byKey(const Key('composer-clip')));
      expect(clip.top - pill.top, clip.left - pill.left);
      expect(pill.bottom - clip.bottom, clip.left - pill.left);
      final send = tester.getRect(find.byKey(const Key('composer-send')));
      expect(pill.right - send.right, clip.left - pill.left);
      expect(send.top - pill.top, clip.left - pill.left);
      expect(find.descendant(of: find.byKey(const Key('composer-pill')), matching: find.byType(Glass)), findsNothing);
    });

    testWidgets('while an answer runs it says Working and offers Stop', (tester) async {
      final repo = repoWith(
        sessions: [sessionOf('c1', running: true)],
        messages: snapshotOf('c1', messages: [messageOf('m1', MessageRole.user, 'Go'), messageOf('a1', MessageRole.assistant, 'Looking', status: MessageStatus.partial, runId: 'r1')]),
      );
      await pumpSessions(tester, repo, const SessionPage(conversationId: 'c1'));
      await openSessionProperties(tester);
      expect(find.descendant(of: find.byKey(const ValueKey('session-status')), matching: find.text('Working')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('properties-close')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const Key('composer-stop')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'run.stop');
    });

    testWidgets('while the dispatcher chose nobody: no composer, the choice of who takes it (work_session.assign)', (tester) async {
      final repo = repoWith(sessions: [sessionOf('c1', dispatch: SessionDispatch.nobody, employeeId: 'orion', employee: 'Orion')], messages: snapshotOf('c1'));
      await pumpSessions(tester, repo, const SessionPage(conversationId: 'c1'));
      expect(find.text('The dispatcher chose nobody. Choose who takes this session.'), findsOneWidget);
      await openSessionProperties(tester);
      expect(find.text('Waiting for your choice'), findsOneWidget);
      expect(find.text('Not chosen yet'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('properties-close')));
      await settleBoard(tester);
      expect(find.byKey(const Key('composer-send')), findsNothing);
      await tester.tap(find.byKey(const ValueKey('session-choose')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('assign-forge')));
      await settleBoard(tester);
      final assign = writesOf(repo).single;
      expect(assign['method'], 'work_session.assign');
      expect(paramsOf(assign), {'conversation_id': 'c1', 'employee_id': 'forge'});
    });

    testWidgets('Properties (behind …) show who, status, where, the run and when, then the files written and the run\'s guidelines', (tester) async {
      final edit = ActivityView(callId: 'e1', input: const {'file_path': '/repo/lib/ios.dart', 'old_string': 'a', 'new_string': 'b'}, offset: 0, ok: true, tool: 'Edit');
      final repo = repoWith(
        sessions: [sessionOf('c1')],
        messages: snapshotOf('c1', messages: [messageOf('m1', MessageRole.user, 'Go'), messageOf('a1', MessageRole.assistant, 'Done.', ordinal: 1, runId: 'r1', activity: [edit])]),
      );
      when(() => repo.guidelinesOfRun('r1')).thenAnswer((_) async => const [RunGuideline(chars: 10, guidelineId: 'g1', level: GuidelineLevel.project, runId: 'r1', title: 'Rust style', version: 2)]);
      await pumpSessions(tester, repo, const SessionPage(conversationId: 'c1'));
      expect(find.byKey(const Key('diff-card')), findsOneWidget, reason: 'an edit shows what changed');
      await openSessionProperties(tester);
      expect(find.text('Forge'), findsWidgets);
      expect(find.text('Waiting for you'), findsOneWidget);
      expect(find.text('Project folder'), findsOneWidget);
      expect(find.text(startedLabel(0)), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('properties-link-Files written')));
      await settleBoard(tester);
      expect(find.text('/repo/lib/ios.dart'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('properties-close')).last);
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('properties-link-Guidelines in this run')));
      await settleBoard(tester);
      expect(find.text('Rust style'), findsOneWidget);
      expect(find.text('v2'), findsOneWidget);
    });

    testWidgets('a failed read shows what failed and Try again', (tester) async {
      final repo = repoWith();
      when(() => repo.workSessions()).thenThrow(const ProtocolException(ErrorCode.internal, 'The server broke.'));
      await pumpSessions(tester, repo, const SessionPage(conversationId: 'c1'));
      expect(find.text('Could not load this session.'), findsOneWidget);
      expect(find.byKey(const ValueKey('session-retry')), findsOneWidget);
    });
  });

  group('a conversation link to a session', () {
    testWidgets('an Inbox Running row of a session (/conversation/:cid) opens the session, not "no longer exists"', (tester) async {
      final repo = repoWith(sessions: [sessionOf('s1')]);
      await pumpSessions(tester, repo, const ConversationPage(conversationId: 's1'));
      expect(find.text('session s1'), findsOneWidget);
      expect(find.text('This conversation no longer exists'), findsNothing);
    });

    testWidgets('a conversation that is not a session still says it no longer exists', (tester) async {
      final repo = repoWith(sessions: [sessionOf('s1')]);
      await pumpSessions(tester, repo, const ConversationPage(conversationId: 'gone'));
      expect(find.byKey(const ValueKey('conversation-gone')), findsOneWidget);
    });

    testWidgets('a failed session read shows what failed and Try again', (tester) async {
      final repo = repoWith();
      when(() => repo.workSessions()).thenThrow(const ProtocolException(ErrorCode.internal, 'The server broke.'));
      await pumpSessions(tester, repo, const ConversationPage(conversationId: 's1'));
      expect(find.byKey(const ValueKey('conversation-load-failed')), findsOneWidget);
      when(() => repo.workSessions()).thenAnswer((_) async => [sessionOf('s1')]);
      await tester.tap(find.text('Try again'));
      await settleBoard(tester);
      expect(find.text('session s1'), findsOneWidget);
    });
  });
}
