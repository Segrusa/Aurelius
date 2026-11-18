import 'dart:async';
import 'dart:convert';

import 'package:aurelius_mobile/app/router.dart';
import 'package:aurelius_mobile/core/hosts/host_store.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/protocol/protocol.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/onboarding/onboarding.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/cupertino.dart' show CupertinoIcons;
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'support/fake_daemon.dart';
import 'team_tab_test.dart' show pathsOf;

class MockRepository extends Mock implements AureliusRepository {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

const secret = 'correct horse battery';
const ada = UserView(id: 'u-1', username: 'ada', createdAtMs: 0);
const marie = UserView(id: 'u-2', username: 'marie', createdAtMs: 1, createdByUser: 'u-1');

void main() {
  group('connection (protocol 5: hello session_token, user.login, user.create)', () {
    late FakeDaemon daemon;
    late MemoryHostStore store;
    late AureliusConnection connection;

    setUp(() async {
      daemon = FakeDaemon();
      store = await storeWithSecret();
      connection = AureliusConnection(host: testHost, store: store, api: FakeApi(), connect: daemon.connect, deviceName: 'Test phone');
    });

    tearDown(() => connection.dispose());

    Future<void> reconnect() async {
      await daemon.current!.drop();
      connection.wake();
      await until(() => !connection.state.isOnline);
      connection.wake();
      await until(() => connection.state.isOnline);
    }

    test('a server without board members: the first one is created, signed in, and the app opens (MU1-20, MU3-02 AC5)', () async {
      connection.start();
      await until(() => connection.state.isOnline);
      expect(connection.state.boardMemberStep, BoardMemberStep.createFirst);
      expect(daemon.hello!.containsKey('session_token'), isFalse);

      final id = await connection.createBoardMember('alex', secret);
      expect(id, 'u-1');
      expect(connection.state.boardMemberStep, BoardMemberStep.ready);
      expect(connection.state.user?.username, 'alex');
      expect(await store.sessionToken(testHost.hostId), isNotNull, reason: 'signed in once more for the token');
    });

    test('the kept session goes with every hello: a reconnect needs no password (MU3-03 AC2)', () async {
      daemon.addUser('u-1', 'alex', secret);
      await store.setSessionToken(testHost.hostId, daemon.sessionFor('u-1'));
      connection.start();
      await until(() => connection.state.isOnline);
      expect(daemon.hello!['session_token'], 'aus_1');
      expect(daemon.hello!.containsKey('user_id'), isFalse, reason: 'protocol 5 has no user_id in the hello');
      expect(connection.state.boardMemberStep, BoardMemberStep.ready);

      await reconnect();
      expect(daemon.sockets.length, greaterThanOrEqualTo(2));
      expect(daemon.hello!['session_token'], 'aus_1');
      expect(connection.state.user?.id, 'u-1');
    });

    test('with board members and no session: the sign-in screen, one generic error, then signed in (MU3-02 AC1–AC3)', () async {
      daemon.addUser('u-1', 'alex', secret);
      connection.start();
      await until(() => connection.state.isOnline);
      expect(connection.state.boardMemberStep, BoardMemberStep.signIn);

      await expectLater(connection.signIn('alex', 'wrong password!'), throwsA(isA<ProtocolException>().having((e) => e.code, 'code', ErrorCode.wrongPassword)));
      expect(connection.state.boardMemberStep, BoardMemberStep.signIn);
      expect(await store.sessionToken(testHost.hostId), isNull);

      final login = await connection.signIn(' alex ', secret);
      expect(login.user.username, 'alex');
      expect(connection.state.boardMemberStep, BoardMemberStep.ready);
      expect(await store.sessionToken(testHost.hostId), login.sessionToken);
      final sent = daemon.frames.lastWhere((f) => f.contains('"user.login"'));
      expect(sent, contains('"device_name":"Test phone"'));
      expect(sent, contains('"username":"alex"'));
    });

    test('too many tries: too_many_attempts with its wait (B9)', () async {
      daemon.addUser('u-1', 'alex', secret);
      connection.start();
      await until(() => connection.state.isOnline);
      for (var i = 0; i < 5; i++) {
        await expectLater(connection.signIn('alex', 'nope nope nope'), throwsA(isA<ProtocolException>()));
      }
      await expectLater(
        connection.signIn('alex', secret),
        throwsA(isA<ProtocolException>().having((e) => e.code, 'code', ErrorCode.tooManyAttempts).having((e) => e.retryAfterMs, 'retryAfterMs', 30000)),
      );
    });

    test('a kept session the server no longer has: forgotten, the sign-in screen (MU3-03 AC3)', () async {
      daemon.addUser('u-9', 'robin', secret);
      await store.setSessionToken(testHost.hostId, 'aus_gone');
      connection.start();
      await until(() => connection.state.isOnline);
      expect(daemon.hello!['session_token'], 'aus_gone');
      expect(connection.state.boardMemberStep, BoardMemberStep.signIn);
      expect(await store.sessionToken(testHost.hostId), isNull);
    });

    test('a second board member: the creator stays signed in (MU1-22 AC1); a taken name is refused (AC2)', () async {
      daemon.addUser('u-1', 'alex', secret);
      await store.setSessionToken(testHost.hostId, daemon.sessionFor('u-1'));
      connection.start();
      await until(() => connection.state.isOnline);

      expect(await connection.createBoardMember('robin', 'another long secret'), 'u-2');
      expect(connection.state.user?.id, 'u-1');
      expect(await store.sessionToken(testHost.hostId), 'aus_1');
      expect(daemon.frames.where((f) => f.contains('"user.login"')), isEmpty);
      await expectLater(connection.createBoardMember('Robin', 'another long secret'), throwsA(isA<ProtocolException>().having((e) => e.code, 'code', ErrorCode.usernameTaken)));
    });

    test('board_member_required: a clear message, and the app goes back to the sign-in screen', () async {
      daemon.addUser('u-1', 'alex', secret);
      await store.setSessionToken(testHost.hostId, daemon.sessionFor('u-1'));
      connection.start();
      await until(() => connection.state.isOnline);
      daemon.current!.user = null; // the server lost this connection's board member
      Object? error;
      try {
        await connection.write({'method': 'ticket.create', 'params': <String, dynamic>{}});
      } catch (e) {
        error = e;
      }
      expect(error, isA<ProtocolException>().having((e) => e.needsBoardMember, 'needsBoardMember', isTrue));
      expect('$error', boardMemberRequiredMessage);
      expect(boardMemberRequiredMessage, contains('Sign in'));
      await until(() => connection.state.boardMemberStep == BoardMemberStep.signIn);
    });

    test('passwords are sent, never kept: not in storage, not in the logs', () async {
      final logged = <String>[];
      final print = debugPrint;
      debugPrint = (message, {wrapWidth}) => logged.add('$message');
      addTearDown(() => debugPrint = print);
      connection.start();
      await until(() => connection.state.isOnline);
      await connection.createBoardMember('alex', secret);
      await reconnect();
      expect(await store.sessionToken(testHost.hostId), isNot(contains(secret)));
      expect(logged.where((l) => l.contains(secret)), isEmpty);
      expect(daemon.frames.where((f) => f.contains(secret)).map((f) => (jsonDecode(f) as Map)['method']), ['user.create', 'user.login'], reason: 'a reconnect resends nothing');
    });

    test('a user.create whose answer never came is not "Not sent": the PC may have made it (Trace M3a m2)', () async {
      connection.start();
      await until(() => connection.state.isOnline);
      daemon.dropNextCommand = true;
      await expectLater(connection.createBoardMember('alex', secret), throwsA(isA<NotSentException>().having((e) => e.sent, 'sent', isTrue)));
      expect(daemon.users.map((u) => u['username']), ['alex'], reason: 'made on the PC');
      connection.wake();
      await until(() => connection.state.isOnline);
      expect(daemon.frames.where((f) => f.contains(secret)), hasLength(1), reason: 'never resent after the reconnect');
    });

    test('offline a user.create is not sent at all', () async {
      await expectLater(connection.createBoardMember('alex', secret), throwsA(isA<NotSentException>().having((e) => e.sent, 'sent', isFalse)));
      expect(daemon.frames.where((f) => f.contains(secret)), isEmpty);
    });

    test('forgetting a server forgets its session', () async {
      await store.setSessionToken(testHost.hostId, 'aus_1');
      await store.remove(testHost.hostId);
      expect(await store.sessionToken(testHost.hostId), isNull);
    });
  });

  group('the gate (impactanalyse §4b.4)', () {
    test('without a session only the onboarding and the server choice are reachable (MU1-20 AC1, MU3-02 AC1, MU3-06)', () {
      for (final step in [BoardMemberStep.createFirst, BoardMemberStep.signIn]) {
        expect(boardMemberRedirect(step, '/home'), '/onboarding');
        expect(boardMemberRedirect(step, '/team'), '/onboarding');
        expect(boardMemberRedirect(step, '/project/p/task/t'), '/onboarding');
        expect(boardMemberRedirect(step, '/account'), '/onboarding');
        expect(boardMemberRedirect(step, '/onboarding'), isNull);
        expect(boardMemberRedirect(step, '/onboarding/new'), isNull);
        expect(boardMemberRedirect(step, '/hosts'), isNull);
        expect(boardMemberRedirect(step, '/pair'), isNull);
      }
    });

    test('with one the onboarding is left; before the first hello nothing is decided', () {
      expect(boardMemberRedirect(BoardMemberStep.ready, '/onboarding'), '/team');
      expect(boardMemberRedirect(BoardMemberStep.ready, '/onboarding/new'), '/team');
      expect(boardMemberRedirect(BoardMemberStep.ready, '/home'), isNull);
      expect(boardMemberRedirect(BoardMemberStep.unknown, '/home'), isNull);
    });

    test('the step follows the hello', () {
      expect(const LinkState(status: LinkStatus.connecting).boardMemberStep, BoardMemberStep.unknown);
      expect(const LinkState(status: LinkStatus.online, boardMembersExist: false).boardMemberStep, BoardMemberStep.createFirst);
      expect(const LinkState(status: LinkStatus.online, boardMembersExist: true).boardMemberStep, BoardMemberStep.signIn);
      const ready = LinkState(status: LinkStatus.online, boardMembersExist: true, user: ada);
      expect(ready.boardMemberStep, BoardMemberStep.ready);
      expect(ready.copyWith(status: LinkStatus.offline).user, ada, reason: 'offline keeps who you are');
      expect(ready.withBoardMember(exists: true).boardMemberStep, BoardMemberStep.signIn);
      expect(ready.withBoardMember(exists: true, user: ada, mustChangePassword: true).boardMemberStep, BoardMemberStep.changePassword);
    });

    test('the app has the onboarding and board member routes', () {
      final router = AppRouter(testHost, boardMember: ValueNotifier(BoardMemberStep.ready));
      addTearDown(router.dispose);
      final paths = pathsOf(router.config.configuration.routes);
      expect(paths, containsAll(['/onboarding', '/onboarding/new', '/account', '/account/sessions', '/account/password', '/account/claude', '/settings/board-members', '/settings/board-members/new', '/settings/board-members/:uid/sessions', '/settings/board-members/:uid/password']));
    });
  });

  group('screens', () {
    late MockRepository repo;
    late MockConnectionBloc link;
    late StreamController<LinkState> links;
    late ValueNotifier<BoardMemberStep> step;
    late StreamController<Event> userEvents;
    var members = <UserView>[];

    void setLink(LinkState state) {
      when(() => link.state).thenReturn(state);
      when(() => repo.linkState).thenReturn(state);
      links.add(state);
      step.value = state.boardMemberStep;
    }

    setUp(() {
      repo = MockRepository();
      link = MockConnectionBloc();
      links = StreamController<LinkState>.broadcast();
      userEvents = StreamController<Event>.broadcast();
      step = ValueNotifier(BoardMemberStep.unknown);
      members = [];
      when(() => repo.host).thenReturn(testHost);
      when(() => repo.boardMembers()).thenAnswer((_) async => members);
      when(() => repo.boardMemberChanges).thenAnswer((_) => userEvents.stream);
      whenListen(link, links.stream, initialState: const LinkState(status: LinkStatus.online));
    });

    tearDown(() {
      links.close();
      userEvents.close();
      step.dispose();
    });

    Future<GoRouter> pump(WidgetTester tester, LinkState initial, {String at = '/team'}) async {
      setLink(initial);
      final router = GoRouter(
        initialLocation: at,
        refreshListenable: step,
        redirect: (context, state) => boardMemberRedirect(step.value, state.matchedLocation),
        routes: [
          GoRoute(path: '/team', builder: (_, _) => const Scaffold(body: Text('page team'))),
          GoRoute(path: '/hosts', builder: (_, _) => const Scaffold(body: Text('page hosts'))),
          GoRoute(path: '/profile', builder: (_, _) => const Scaffold(body: Text('page profile'))),
          GoRoute(path: '/onboarding', builder: (_, _) => const OnboardingPage(), routes: [
            GoRoute(path: 'new', builder: (_, _) => const NewBoardMemberPage()),
          ]),
          GoRoute(path: '/account', builder: (_, _) => const Scaffold(body: Text('page account'))),
          GoRoute(path: '/settings/board-members', builder: (_, _) => const BoardMembersPage(), routes: [
            GoRoute(path: 'new', builder: (_, _) => const NewBoardMemberPage()),
          ]),
        ],
      );
      addTearDown(router.dispose);
      final company = MockCompanyBloc();
      whenListen(company, const Stream<CompanyState>.empty(), initialState: const CompanyState(loading: false));
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: MultiBlocProvider(
          providers: [BlocProvider<ConnectionBloc>.value(value: link), BlocProvider<CompanyBloc>.value(value: company)],
          child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router),
        ),
      ));
      await tester.pumpAndSettle();
      return router;
    }

    Future<void> fill(WidgetTester tester, String username, String password, [String? repeat]) async {
      await tester.enterText(find.byKey(const ValueKey('board-member-username')), username);
      await tester.enterText(find.byKey(const ValueKey('board-member-password')), password);
      await tester.enterText(find.byKey(const ValueKey('board-member-repeat')), repeat ?? password);
      await tester.pump();
    }

    Future<void> tapCreate(WidgetTester tester) async {
      await tester.ensureVisible(find.byKey(const ValueKey('board-member-create')));
      await tester.tap(find.byKey(const ValueKey('board-member-create')));
      await tester.pumpAndSettle();
    }

    testWidgets('first start: only "Create the first board member"; created → the app and a welcome (MU1-20)', (tester) async {
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: false));
      expect(find.text('Create the first board member'), findsWidgets);
      expect(find.text('page team'), findsNothing);
      expect(find.text('Choose another server'), findsOneWidget);

      when(() => repo.createBoardMember('ada', secret)).thenAnswer((_) async {
        setLink(const LinkState(status: LinkStatus.online, boardMembersExist: true, user: ada));
        return 'u-1';
      });
      await fill(tester, 'ada', secret);
      expect(find.byIcon(CupertinoIcons.checkmark_alt), findsNWidgets(2), reason: 'both rules hold');
      await tapCreate(tester);
      verify(() => repo.createBoardMember('ada', secret)).called(1);
      expect(find.text('page team'), findsOneWidget);
      expect(find.text('Welcome, ada.'), findsOneWidget);
    });

    testWidgets('the rules are checked before sending; the server\'s reasons show at the field (MU1-20 AC3)', (tester) async {
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: false));
      await fill(tester, 'se', 'short', 'other');
      await tapCreate(tester);
      verifyNever(() => repo.createBoardMember(any(), any()));
      expect(find.text('The passwords differ.'), findsOneWidget);
      expect(find.text(BoardMemberRules.usernameRule), findsNWidgets(2), reason: 'the rule, and as the error');

      when(() => repo.createBoardMember(any(), any())).thenThrow(const ProtocolException(ErrorCode.usernameTaken, 'This username is taken.'));
      await fill(tester, 'ada', secret);
      await tapCreate(tester);
      expect(find.text('This username is taken.'), findsOneWidget);
      expect(find.text('Create the first board member'), findsWidgets);
      expect(find.descendant(of: find.byType(SnackBar), matching: find.textContaining(secret)), findsNothing, reason: 'the password is never in a toast');
    });

    test('the password rule counts characters as the server does (Trace M3a m4)', () {
      expect(BoardMemberRules.password('twelve chars'), isTrue);
      expect(BoardMemberRules.password(' ' * 12), isFalse, reason: 'only spaces');
      expect(BoardMemberRules.passwordProblem(' ' * 12), 'A password of only spaces is too weak.');
      expect(BoardMemberRules.password('😀' * 6), isFalse, reason: 'six characters, twelve UTF-16 units');
      expect(BoardMemberRules.password('😀' * 12), isTrue);
      expect(BoardMemberRules.password('a' * 1024), isTrue);
      expect(BoardMemberRules.password('a' * 1025), isFalse);
      expect(BoardMemberRules.passwordProblem('a' * 1025), 'A password has at most 1024 characters.');
    });

    testWidgets('weak_password shows the server\'s reason, not always "At least 12 characters"', (tester) async {
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: false));
      when(() => repo.createBoardMember(any(), any())).thenThrow(const ProtocolException(ErrorCode.weakPassword, 'A password has at most 1024 characters.'));
      await fill(tester, 'ada', secret);
      await tapCreate(tester);
      expect(find.text('A password has at most 1024 characters.'), findsOneWidget);
    });

    testWidgets('a create whose answer was lost says so, without "Not sent"', (tester) async {
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: false));
      when(() => repo.createBoardMember(any(), any())).thenThrow(const NotSentException('k', sent: true));
      await fill(tester, 'ada', secret);
      await tapCreate(tester);
      expect(find.text('The connection dropped. If ada was made, sign in as ada.'), findsOneWidget);
      expect(find.textContaining('Not sent'), findsNothing);
    });

    testWidgets('"Choose another server" stays reachable', (tester) async {
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: false));
      await tester.ensureVisible(find.byKey(const ValueKey('onboarding-other-server')));
      await tester.tap(find.byKey(const ValueKey('onboarding-other-server')));
      await tester.pumpAndSettle();
      expect(find.text('page hosts'), findsOneWidget);
    });

    testWidgets('Settings → Board members: the list with "you", and a second board member (MU1-22)', (tester) async {
      members = [ada];
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: true, user: ada), at: '/settings/board-members');
      expect(find.text('you'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('board-members-new')));
      await tester.pumpAndSettle();
      when(() => repo.createBoardMember('marie', 'marie her secret')).thenAnswer((_) async {
        members = [ada, marie];
        userEvents.add(Event.fromJson({'seq': 12, 'kind': 'user.created', 'aggregate_id': 'u-2', 'at_ms': 0}));
        return 'u-2';
      });
      await fill(tester, 'marie', 'marie her secret');
      await tapCreate(tester);
      expect(find.text('marie is a board member now.'), findsOneWidget);
      expect(find.text('Board members'), findsWidgets, reason: 'back on the list');
      expect(find.text('marie'), findsOneWidget);
      expect(find.text('you'), findsOneWidget, reason: 'the creator stays itself');
    });

    testWidgets('no session any more while in the app: back to the sign-in screen', (tester) async {
      members = [ada];
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: true, user: ada));
      expect(find.text('page team'), findsOneWidget);
      setLink(const LinkState(status: LinkStatus.online, boardMembersExist: true));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('sign-in')), findsOneWidget);
    });
  });
}
