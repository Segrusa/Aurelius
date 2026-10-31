import 'dart:async';

import 'package:aurelius_mobile/app/router.dart';
import 'package:aurelius_mobile/core/hosts/host_store.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/protocol/protocol.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/more/more_page.dart';
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
  group('connection (S5: hello user_id, user.use, user.create)', () {
    late FakeDaemon daemon;
    late MemoryHostStore store;
    late AureliusConnection connection;

    setUp(() async {
      daemon = FakeDaemon();
      store = await storeWithSecret();
      connection = AureliusConnection(host: testHost, store: store, api: FakeApi(), connect: daemon.connect);
    });

    tearDown(() => connection.dispose());

    Future<void> reconnect() async {
      await daemon.current!.drop();
      connection.wake();
      await until(() => !connection.state.isOnline);
      connection.wake();
      await until(() => connection.state.isOnline);
    }

    test('a server without board members: the first one is created and the app opens (MU1-20 AC1/AC2)', () async {
      connection.start();
      await until(() => connection.state.isOnline);
      expect(connection.state.boardMemberStep, BoardMemberStep.createFirst);
      expect(daemon.hello!.containsKey('user_id'), isFalse);

      final id = await connection.createBoardMember('ada', secret);
      expect(id, 'u-1');
      expect(connection.state.boardMemberStep, BoardMemberStep.ready);
      expect(connection.state.user?.username, 'ada');
      expect(await store.boardMember(testHost.hostId), 'u-1', reason: 'remembered for the next hello');
    });

    test('the remembered board member goes with every hello: a reconnect needs no password', () async {
      daemon.addUser('u-1', 'ada', secret);
      await store.setBoardMember(testHost.hostId, 'u-1');
      connection.start();
      await until(() => connection.state.isOnline);
      expect(daemon.hello!['user_id'], 'u-1');
      expect(connection.state.boardMemberStep, BoardMemberStep.ready);

      await reconnect();
      expect(daemon.sockets.length, greaterThanOrEqualTo(2));
      expect(daemon.hello!['user_id'], 'u-1');
      expect(connection.state.user?.id, 'u-1');
    });

    test('with board members and none chosen: the choice, one password check, then remembered (MU1-21 AC2, B8)', () async {
      daemon.addUser('u-1', 'ada', secret);
      connection.start();
      await until(() => connection.state.isOnline);
      expect(connection.state.boardMemberStep, BoardMemberStep.choose);

      await expectLater(connection.useBoardMember('u-1', 'wrong password!'), throwsA(isA<ProtocolException>().having((e) => e.code, 'code', ErrorCode.wrongPassword)));
      expect(connection.state.boardMemberStep, BoardMemberStep.choose);
      expect(await store.boardMember(testHost.hostId), isNull);

      final user = await connection.useBoardMember('u-1', secret);
      expect(user.username, 'ada');
      expect(connection.state.boardMemberStep, BoardMemberStep.ready);
      expect(await store.boardMember(testHost.hostId), 'u-1');
    });

    test('too many tries say so', () async {
      daemon.addUser('u-1', 'ada', secret);
      connection.start();
      await until(() => connection.state.isOnline);
      for (var i = 0; i < 5; i++) {
        await expectLater(connection.useBoardMember('u-1', 'nope nope nope'), throwsA(isA<ProtocolException>()));
      }
      await expectLater(connection.useBoardMember('u-1', secret), throwsA(isA<ProtocolException>().having((e) => e.code, 'code', ErrorCode.tooManyAttempts)));
    });

    test('a remembered board member the server does not know: forgotten, back to the choice', () async {
      daemon.addUser('u-9', 'marie', secret);
      await store.setBoardMember(testHost.hostId, 'u-1');
      connection.start();
      await until(() => connection.state.isOnline);
      expect(daemon.hello!['user_id'], 'u-1');
      expect(connection.state.boardMemberStep, BoardMemberStep.choose);
      expect(await store.boardMember(testHost.hostId), isNull);
    });

    test('a second board member: the creator stays itself (MU1-22 AC1); a taken name is refused (AC2)', () async {
      daemon.addUser('u-1', 'ada', secret);
      await store.setBoardMember(testHost.hostId, 'u-1');
      connection.start();
      await until(() => connection.state.isOnline);

      expect(await connection.createBoardMember('marie', 'another long secret'), 'u-2');
      expect(connection.state.user?.id, 'u-1');
      expect(await store.boardMember(testHost.hostId), 'u-1');
      await expectLater(connection.createBoardMember('Marie', 'another long secret'), throwsA(isA<ProtocolException>().having((e) => e.code, 'code', ErrorCode.usernameTaken)));
    });

    test('switching is a new user.use on the same connection', () async {
      daemon
        ..addUser('u-1', 'ada', secret)
        ..addUser('u-2', 'marie', 'marie her secret');
      await store.setBoardMember(testHost.hostId, 'u-1');
      connection.start();
      await until(() => connection.state.isOnline);
      await connection.useBoardMember('u-2', 'marie her secret');
      expect(connection.state.user?.username, 'marie');
      expect(await store.boardMember(testHost.hostId), 'u-2');
      expect(daemon.sockets, hasLength(1));
    });

    test('board_member_required: a clear message, and the app goes back to the choice', () async {
      daemon.addUser('u-1', 'ada', secret);
      await store.setBoardMember(testHost.hostId, 'u-1');
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
      expect(boardMemberRequiredMessage, contains('Choose who you are'));
      expect(connection.state.boardMemberStep, BoardMemberStep.choose);
    });

    test('passwords are sent, never kept: not in storage, not in the logs', () async {
      final logged = <String>[];
      final print = debugPrint;
      debugPrint = (message, {wrapWidth}) => logged.add('$message');
      addTearDown(() => debugPrint = print);
      connection.start();
      await until(() => connection.state.isOnline);
      await connection.createBoardMember('ada', secret);
      await reconnect();
      expect(await store.boardMember(testHost.hostId), 'u-1');
      expect(logged.where((l) => l.contains(secret)), isEmpty);
      expect(daemon.frames.where((f) => f.contains(secret)), hasLength(1), reason: 'only the user.create itself; a reconnect resends nothing');
      expect(daemon.frames.firstWhere((f) => f.contains(secret)), contains('"user.create"'));
    });

    test('a user.create whose answer never came is not "Not sent": the PC may have made it (Trace M3a m2)', () async {
      connection.start();
      await until(() => connection.state.isOnline);
      daemon.dropNextCommand = true;
      await expectLater(connection.createBoardMember('ada', secret), throwsA(isA<NotSentException>().having((e) => e.sent, 'sent', isTrue)));
      expect(daemon.users.map((u) => u['username']), ['ada'], reason: 'made on the PC');
      connection.wake();
      await until(() => connection.state.isOnline);
      expect(daemon.frames.where((f) => f.contains(secret)), hasLength(1), reason: 'never resent after the reconnect');
    });

    test('offline a user.create is not sent at all', () async {
      await expectLater(connection.createBoardMember('ada', secret), throwsA(isA<NotSentException>().having((e) => e.sent, 'sent', isFalse)));
      expect(daemon.frames.where((f) => f.contains(secret)), isEmpty);
    });

    test('forgetting a server forgets its board member', () async {
      await store.setBoardMember(testHost.hostId, 'u-1');
      await store.remove(testHost.hostId);
      expect(await store.boardMember(testHost.hostId), isNull);
    });
  });

  group('the gate (impactanalyse §4b.4)', () {
    test('without a board member only the onboarding and the server choice are reachable (MU1-20 AC1)', () {
      for (final step in [BoardMemberStep.createFirst, BoardMemberStep.choose]) {
        expect(boardMemberRedirect(step, '/home'), '/onboarding');
        expect(boardMemberRedirect(step, '/team'), '/onboarding');
        expect(boardMemberRedirect(step, '/project/p/task/t'), '/onboarding');
        expect(boardMemberRedirect(step, '/switch-board-member'), '/onboarding');
        expect(boardMemberRedirect(step, '/onboarding'), isNull);
        expect(boardMemberRedirect(step, '/onboarding/new'), isNull);
        expect(boardMemberRedirect(step, '/hosts'), isNull);
        expect(boardMemberRedirect(step, '/pair'), isNull);
      }
    });

    test('with one the onboarding is left; before the first hello nothing is decided', () {
      expect(boardMemberRedirect(BoardMemberStep.ready, '/onboarding'), '/home');
      expect(boardMemberRedirect(BoardMemberStep.ready, '/onboarding/new'), '/home');
      expect(boardMemberRedirect(BoardMemberStep.ready, '/home'), isNull);
      expect(boardMemberRedirect(BoardMemberStep.unknown, '/home'), isNull);
    });

    test('the step follows the hello', () {
      expect(const LinkState(status: LinkStatus.connecting).boardMemberStep, BoardMemberStep.unknown);
      expect(const LinkState(status: LinkStatus.online, boardMembersExist: false).boardMemberStep, BoardMemberStep.createFirst);
      expect(const LinkState(status: LinkStatus.online, boardMembersExist: true).boardMemberStep, BoardMemberStep.choose);
      const ready = LinkState(status: LinkStatus.online, boardMembersExist: true, user: ada);
      expect(ready.boardMemberStep, BoardMemberStep.ready);
      expect(ready.copyWith(status: LinkStatus.offline).user, ada, reason: 'offline keeps who you are');
      expect(ready.withBoardMember(exists: true).boardMemberStep, BoardMemberStep.choose);
    });

    test('the app has the onboarding and board member routes', () {
      final router = AppRouter(testHost, boardMember: ValueNotifier(BoardMemberStep.ready));
      addTearDown(router.dispose);
      final paths = pathsOf(router.config.configuration.routes);
      expect(paths, containsAll(['/onboarding', '/onboarding/new', '/switch-board-member', '/settings/board-members', '/settings/board-members/new']));
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

    Future<GoRouter> pump(WidgetTester tester, LinkState initial, {String at = '/home'}) async {
      setLink(initial);
      final router = GoRouter(
        initialLocation: at,
        refreshListenable: step,
        redirect: (context, state) => boardMemberRedirect(step.value, state.matchedLocation),
        routes: [
          GoRoute(path: '/home', builder: (_, _) => const Scaffold(body: Text('page home'))),
          GoRoute(path: '/hosts', builder: (_, _) => const Scaffold(body: Text('page hosts'))),
          GoRoute(path: '/more', builder: (_, _) => const MorePage()),
          GoRoute(path: '/onboarding', builder: (_, _) => const OnboardingPage(), routes: [
            GoRoute(path: 'new', builder: (_, _) => const NewBoardMemberPage()),
          ]),
          GoRoute(path: '/switch-board-member', builder: (_, _) => const ChooseBoardMemberPage(switching: true)),
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
      expect(find.text('page home'), findsNothing);
      expect(find.text('Choose another server'), findsOneWidget);

      when(() => repo.createBoardMember('ada', secret)).thenAnswer((_) async {
        setLink(const LinkState(status: LinkStatus.online, boardMembersExist: true, user: ada));
        return 'u-1';
      });
      await fill(tester, 'ada', secret);
      expect(find.byIcon(CupertinoIcons.checkmark_alt), findsNWidgets(2), reason: 'both rules hold');
      await tapCreate(tester);
      verify(() => repo.createBoardMember('ada', secret)).called(1);
      expect(find.text('page home'), findsOneWidget);
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
      expect(find.text('The connection dropped. If ada is in the list, choose it.'), findsOneWidget);
      expect(find.textContaining('Not sent'), findsNothing);
    });

    testWidgets('"Choose another server" stays reachable', (tester) async {
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: false));
      await tester.ensureVisible(find.byKey(const ValueKey('onboarding-other-server')));
      await tester.tap(find.byKey(const ValueKey('onboarding-other-server')));
      await tester.pumpAndSettle();
      expect(find.text('page hosts'), findsOneWidget);
    });

    testWidgets("with board members: Who's working here?, the password once, then the app (MU1-21 AC2, B8)", (tester) async {
      members = [ada, marie];
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: true));
      expect(find.text("Who's working here?"), findsWidgets);
      expect(find.text('ada'), findsOneWidget);
      expect(find.text('marie'), findsOneWidget);

      when(() => repo.useBoardMember('u-2', 'wrong')).thenThrow(const ProtocolException(ErrorCode.wrongPassword, 'That board member or password is wrong.'));
      when(() => repo.useBoardMember('u-2', 'marie her secret')).thenAnswer((_) async {
        setLink(const LinkState(status: LinkStatus.online, boardMembersExist: true, user: marie));
        return marie;
      });
      await tester.tap(find.byKey(const ValueKey('board-member-u-2')));
      await tester.pumpAndSettle();
      expect(find.text('This device remembers you. Full sign-in comes later.'), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('board-member-check-password')), 'wrong');
      await tester.tap(find.byKey(const ValueKey('board-member-check')));
      await tester.pumpAndSettle();
      expect(find.text('Wrong password.'), findsOneWidget);

      await tester.enterText(find.byKey(const ValueKey('board-member-check-password')), 'marie her secret');
      await tester.tap(find.byKey(const ValueKey('board-member-check')));
      await tester.pumpAndSettle();
      expect(find.text('page home'), findsOneWidget);
      expect(find.text('Welcome, marie.'), findsOneWidget);
    });

    testWidgets('a board member added elsewhere shows up in the choice', (tester) async {
      members = [ada];
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: true));
      expect(find.text('marie'), findsNothing);
      members = [ada, marie];
      userEvents.add(Event.fromJson({'seq': 12, 'kind': 'user.created', 'aggregate_id': 'u-2', 'at_ms': 0}));
      await tester.pumpAndSettle();
      expect(find.text('marie'), findsOneWidget);
    });

    testWidgets('New board member from the choice: this phone becomes that board member', (tester) async {
      members = [ada];
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: true));
      await tester.tap(find.byKey(const ValueKey('board-member-new')));
      await tester.pumpAndSettle();
      expect(find.text('New board member'), findsWidgets);
      when(() => repo.createBoardMember('marie', 'marie her secret')).thenAnswer((_) async {
        setLink(const LinkState(status: LinkStatus.online, boardMembersExist: true, user: marie));
        return 'u-2';
      });
      await fill(tester, 'marie', 'marie her secret');
      await tapCreate(tester);
      expect(find.text('page home'), findsOneWidget);
    });

    testWidgets('More shows who you are and switches (MU1-21 AC4)', (tester) async {
      members = [ada, marie];
      final router = await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: true, user: ada), at: '/more');
      expect(find.byKey(const ValueKey('more-board-member')), findsOneWidget);
      expect(find.text('ada'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('more-board-member')));
      await tester.pumpAndSettle();
      expect(router.state.matchedLocation, '/switch-board-member');
      expect(find.text('you'), findsOneWidget);

      when(() => repo.useBoardMember('u-2', 'marie her secret')).thenAnswer((_) async {
        setLink(const LinkState(status: LinkStatus.online, boardMembersExist: true, user: marie));
        return marie;
      });
      await tester.tap(find.byKey(const ValueKey('board-member-u-2')));
      await tester.pumpAndSettle();
      await tester.enterText(find.byKey(const ValueKey('board-member-check-password')), 'marie her secret');
      await tester.tap(find.byKey(const ValueKey('board-member-check')));
      await tester.pumpAndSettle();
      expect(find.text('You are marie now.'), findsOneWidget);
      expect(router.state.matchedLocation, '/more');
      expect(find.text('marie'), findsOneWidget);
    });

    testWidgets('Settings → Board members: the list with "you", and a second board member (MU1-22)', (tester) async {
      members = [ada];
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: true, user: ada), at: '/settings/board-members');
      expect(find.text('you'), findsOneWidget);
      expect(find.textContaining('Delete'), findsNothing, reason: 'no delete or reset in Fase 1 (OV-14)');
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

    testWidgets('board_member_required while in the app: back to the choice', (tester) async {
      members = [ada];
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: true, user: ada));
      expect(find.text('page home'), findsOneWidget);
      setLink(const LinkState(status: LinkStatus.online, boardMembersExist: true));
      await tester.pumpAndSettle();
      expect(find.text("Who's working here?"), findsWidgets);
    });
  });
}
