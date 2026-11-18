import 'package:aurelius_mobile/core/net/connection.dart' show LinkState, LinkStatus;
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/widgets/widgets.dart' show StatusDot;
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/protocol/protocol.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/company_org/employee_page.dart' show ConversationPage;
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:aurelius_mobile/features/sessions/new_session_page.dart';
import 'package:aurelius_mobile/features/sessions/session_page.dart';
import 'package:aurelius_mobile/features/sessions/sessions_page.dart';
import 'package:bloc_test/bloc_test.dart';
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

WorkSessionView sessionOf(String id, {String title = 'Fix the login', int at = 0, bool running = false, SessionDispatch? dispatch, String employeeId = 'forge', String employee = 'Forge'}) => WorkSessionView(
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
    );

MockRepository repoWith({List<WorkSessionView> sessions = const [], LinkState? link, MessagesSnapshot? messages}) {
  final repo = boardRepository(link: link ?? sessionsLink, messages: messages);
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

/// [page] under a router that knows where the session pages lead.
Future<GoRouter> pumpSessions(WidgetTester tester, MockRepository repo, Widget page, {LinkState? link}) async {
  tester.view.physicalSize = const Size(390, 844) * 3;
  tester.view.devicePixelRatio = 3;
  addTearDown(tester.view.reset);
  final state = link ?? repo.linkState;
  final company = MockCompanyBloc();
  whenListen(company, const Stream<CompanyState>.empty(), initialState: companyWith());
  final connection = MockConnectionBloc();
  whenListen(connection, const Stream<LinkState>.empty(), initialState: state);
  final inbox = MockInboxBloc();
  whenListen(inbox, const Stream<InboxState>.empty(), initialState: const InboxState(loading: false));
  final runtime = MockRuntimeBloc();
  whenListen(runtime, const Stream<RuntimeState>.empty(), initialState: const RuntimeState());
  final router = GoRouter(initialLocation: '/', routes: [
    GoRoute(path: '/', builder: (_, _) => page),
    GoRoute(path: '/sessions/new', builder: (_, _) => const Scaffold(body: Text('page new session'))),
    GoRoute(path: '/session/:cid', builder: (_, s) => Scaffold(body: Text('session ${s.pathParameters['cid']}'))),
    GoRoute(path: '/board/new', builder: (_, _) => const Scaffold(body: Text('page new ticket'))),
  ]);
  addTearDown(router.dispose);
  await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
    value: repo,
    child: MultiBlocProvider(
      providers: [
        BlocProvider<CompanyBloc>.value(value: company),
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
    testWidgets('lists work_session.list newest first with who, project and the dots; the header counts who works', (tester) async {
      final repo = repoWith(sessions: [
        sessionOf('c1', title: 'Old and quiet', at: 1),
        sessionOf('c2', title: 'Busy one', at: 3, running: true),
        sessionOf('c3', title: 'Nobody chosen', at: 2, dispatch: SessionDispatch.nobody, employee: 'Orion', employeeId: 'orion'),
      ]);
      await pumpSessions(tester, repo, const WorkSessionsPage());
      expect(find.text('Sessions'), findsOneWidget);
      expect(find.text('1 working'), findsOneWidget);
      final busy = tester.getTopLeft(find.text('Busy one')).dy;
      final nobody = tester.getTopLeft(find.text('Nobody chosen')).dy;
      final old = tester.getTopLeft(find.text('Old and quiet')).dy;
      expect(busy < nobody && nobody < old, isTrue, reason: 'newest activity first');
      expect(find.descendant(of: find.byKey(const ValueKey('session-c2')), matching: find.byKey(const ValueKey('session-working'))), findsOneWidget);
      expect(find.descendant(of: find.byKey(const ValueKey('session-c3')), matching: find.byKey(const ValueKey('session-needs-you'))), findsOneWidget);
      expect(find.descendant(of: find.byKey(const ValueKey('session-c1')), matching: find.byType(StatusDot)), findsNothing);
      expect(find.textContaining('Forge · Atlas'), findsNWidgets(2));
      expect(find.textContaining('Nobody yet · Atlas'), findsOneWidget);
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
      await tester.tap(find.byKey(const ValueKey('sessions-new')));
      await settleBoard(tester);
      expect(find.text('page new ticket'), findsOneWidget);
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

    testWidgets('+ opens New session', (tester) async {
      final repo = repoWith();
      await pumpSessions(tester, repo, const WorkSessionsPage());
      await tester.tap(find.byKey(const ValueKey('sessions-new')));
      await settleBoard(tester);
      expect(find.text('page new session'), findsOneWidget);
    });
  });

  group('New session', () {
    Future<void> choose(WidgetTester tester, String row, String option) async {
      await tester.tap(find.byKey(ValueKey('new-session-$row')));
      await settleBoard(tester);
      await tester.tap(find.byKey(ValueKey(option)));
      await settleBoard(tester);
    }

    testWidgets('Who = Dispatcher starts with work_session.dispatch and opens the new session', (tester) async {
      final repo = repoWith();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultWorkSessionDispatching(conversationId: 'c9', messageId: 'm', projectId: 'p1')));
      await pumpSessions(tester, repo, const NewSessionPage());
      expect(find.text('New session'), findsOneWidget);
      expect(find.text('Atlas'), findsOneWidget, reason: 'the only open project is chosen');
      expect(find.text('Dispatcher'), findsOneWidget, reason: 'with session_dispatch the dispatcher chooses unless told');
      await tester.enterText(find.byKey(const ValueKey('new-session-text')), 'Fix the login page');
      await choose(tester, 'model', 'pick-Model-default');
      await choose(tester, 'effort', 'pick-Effort-high');
      await choose(tester, 'mode', 'pick-Mode-plan');
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

    testWidgets('a server without work sessions gets New ticket instead', (tester) async {
      final repo = repoWith(link: onlineLink);
      await pumpSessions(tester, repo, const NewSessionPage());
      expect(find.text('New ticket'), findsOneWidget);
      expect(find.byKey(const ValueKey('new-session-text')), findsNothing);
    });
  });

  group('a session', () {
    testWidgets('shows its title, who works where and the status; a message goes in with chat.send', (tester) async {
      final repo = repoWith(sessions: [sessionOf('c1')], messages: snapshotOf('c1', messages: [messageOf('m1', MessageRole.user, 'Fix the login')]));
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultChatMessageSent(conversationId: 'c1', messageId: 'm2', runId: 'r2')));
      await pumpSessions(tester, repo, const SessionPage(conversationId: 'c1'));
      expect(find.text('Fix the login'), findsWidgets);
      expect(find.text('Forge · Atlas · project folder'), findsOneWidget);
      expect(find.text('Waiting for you'), findsOneWidget);
      await tester.enterText(find.byType(TextField).last, 'Also the logout');
      await tester.pump();
      await tester.tap(find.byKey(const Key('composer-send')));
      await settleBoard(tester);
      final send = writesOf(repo).single;
      expect(send['method'], 'chat.send');
      expect(paramsOf(send)['conversation_id'], 'c1');
      expect(paramsOf(send)['text'], 'Also the logout');
    });

    testWidgets('while an answer runs it says Working and offers Stop', (tester) async {
      final repo = repoWith(
        sessions: [sessionOf('c1', running: true)],
        messages: snapshotOf('c1', messages: [messageOf('m1', MessageRole.user, 'Go'), messageOf('a1', MessageRole.assistant, 'Looking', status: MessageStatus.partial, runId: 'r1')]),
      );
      await pumpSessions(tester, repo, const SessionPage(conversationId: 'c1'));
      expect(find.text('Working'), findsOneWidget);
      await tester.tap(find.byKey(const Key('composer-stop')));
      await settleBoard(tester);
      expect(writesOf(repo).single['method'], 'run.stop');
    });

    testWidgets('while the dispatcher chose nobody: no composer, the choice of who takes it (work_session.assign)', (tester) async {
      final repo = repoWith(sessions: [sessionOf('c1', dispatch: SessionDispatch.nobody, employeeId: 'orion', employee: 'Orion')], messages: snapshotOf('c1'));
      await pumpSessions(tester, repo, const SessionPage(conversationId: 'c1'));
      expect(find.text('Waiting for your choice'), findsOneWidget);
      expect(find.text('The dispatcher chose nobody. Choose who takes this session.'), findsOneWidget);
      expect(find.byKey(const Key('composer-send')), findsNothing);
      await tester.tap(find.byKey(const ValueKey('session-choose')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('assign-forge')));
      await settleBoard(tester);
      final assign = writesOf(repo).single;
      expect(assign['method'], 'work_session.assign');
      expect(paramsOf(assign), {'conversation_id': 'c1', 'employee_id': 'forge'});
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
