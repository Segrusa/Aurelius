import 'dart:async';
import 'dart:convert';

import 'package:aurelius_mobile/core/hosts/host_store.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/tickets.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/account/account.dart';
import 'package:aurelius_mobile/features/account/claude_login.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/onboarding/onboarding.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'support/builders.dart';
import 'support/fake_daemon.dart';
import 'support/tickets.dart';

// Fase 3 on the phone (AUR-MU-03-M1): protocol 5 sign-in, sessions, the gate,
// the own Claude login and tickets that wait for a login or a limit. Neutral
// people only: Alex and Robin, test@example.com.

const alexPassword = 'alex long password';
const robinPassword = 'robin long password';
const alex = UserView(id: 'u-1', username: 'alex', createdAtMs: 0);
const robin = UserView(id: 'u-2', username: 'robin', createdAtMs: 1, createdByUser: 'u-1');
const alexIn = LinkState(status: LinkStatus.online, boardMembersExist: true, user: alex, claudeLogin: ClaudeLoginView(userId: 'u-1', status: ClaudeLoginStatus.linked, background: false, email: 'test@example.com', plan: 'max'));
const alexNoLogin = LinkState(status: LinkStatus.online, boardMembersExist: true, user: alex, claudeLogin: ClaudeLoginView(userId: 'u-1', status: ClaudeLoginStatus.missing, background: false));

/// A code of the shape Claude shows; never real.
const fakeCode = 'fake-code-0000#fake-state-0000';

void main() {
  setUpAll(() {
    registerFallbackValue(<String, dynamic>{});
    registerFallbackValue(const ClaudeLoginView(userId: 'u-0', status: ClaudeLoginStatus.missing, background: false));
  });

  group('connection (protocol 5)', () {
    late FakeDaemon daemon;
    late MemoryHostStore store;
    late AureliusConnection connection;

    setUp(() async {
      daemon = FakeDaemon()
        ..addUser('u-1', 'alex', alexPassword)
        ..addUser('u-2', 'robin', robinPassword);
      store = await storeWithSecret();
      connection = AureliusConnection(host: testHost, store: store, api: FakeApi(), connect: daemon.connect, deviceName: 'Test phone');
    });

    tearDown(() => connection.dispose());

    Future<void> signedIn() async {
      await store.setSessionToken(testHost.hostId, daemon.sessionFor('u-1'));
      connection.start();
      await until(() => connection.state.isOnline && connection.state.user != null);
    }

    test('MU3-04 AC2 / MU3-06 AC3: signed_out ends the session at once: token gone, sign-in screen with the notice', () async {
      await signedIn();
      await daemon.signOut(daemon.current!, 'revoked');
      await until(() => connection.state.boardMemberStep == BoardMemberStep.signIn);
      expect(connection.state.signedOut, SignOutReason.revoked);
      expect(await store.sessionToken(testHost.hostId), isNull);
      connection.wake();
      await until(() => daemon.sockets.length >= 2 && connection.state.isOnline);
      expect(daemon.hello!.containsKey('session_token'), isFalse, reason: 'the next hello has no session');
      expect(connection.state.signedOut, SignOutReason.revoked, reason: 'the notice waits across the reconnect');
      connection.dismissSignedOut();
      expect(connection.state.signedOut, isNull);
    });

    test('MU3-04 AC3: Sign out ends this phone\'s session, without a notice', () async {
      await signedIn();
      await connection.signOut();
      expect(daemon.frames.any((f) => f.contains('"user.logout"')), isTrue);
      expect(daemon.sessions, isEmpty);
      expect(await store.sessionToken(testHost.hostId), isNull);
      expect(connection.state.boardMemberStep, BoardMemberStep.signIn);
      expect(connection.state.signedOut, isNull);
    });

    test('MU3-06 AC1: unauthenticated on any call forgets the token and goes to the sign-in screen', () async {
      await signedIn();
      daemon.current!.user = null;
      await expectLater(connection.call({'method': 'session.list'}), throwsA(isA<ProtocolException>().having((e) => e.code, 'code', ErrorCode.unauthenticated)));
      await until(() => connection.state.boardMemberStep == BoardMemberStep.signIn);
      expect(await store.sessionToken(testHost.hostId), isNull);
      expect(connection.state.signedOut, SignOutReason.expired);
    });

    test('MU3-05 / B10: a temporary password keeps the phone on its change; changed, the app opens', () async {
      daemon.mustChange.add('u-2');
      connection.start();
      await until(() => connection.state.isOnline);
      final login = await connection.signIn('robin', robinPassword);
      expect(login.mustChangePassword, isTrue);
      expect(connection.state.boardMemberStep, BoardMemberStep.changePassword);
      await connection.setPassword(newPassword: 'robin new long password', oldPassword: robinPassword);
      expect(connection.state.boardMemberStep, BoardMemberStep.ready);
      final sent = daemon.frames.lastWhere((f) => f.contains('user.set_password'));
      expect(sent, isNot(contains('"user_id"')), reason: 'the own password: no user id');
    });

    test('M2 smoke run: the events wait for a session (the gate refuses health and event.subscribe without one)', () async {
      daemon.mustChange.add('u-2');
      connection.start();
      await until(() => connection.state.isOnline);
      expect(connection.state.boardMemberStep, BoardMemberStep.signIn, reason: 'online at the sign-in, not offline on "Sign in first."');
      expect(daemon.subscribes, 0);
      await connection.signIn('robin', robinPassword);
      expect(daemon.subscribes, 0, reason: 'a temporary password: still only its change');
      await connection.setPassword(newPassword: 'robin new long password', oldPassword: robinPassword);
      expect(daemon.subscribes, 1);
      expect(connection.state.boardMemberStep, BoardMemberStep.ready);
    });

    test('password_change_required on a request: back to the change of the password', () async {
      await signedIn();
      daemon.refuse['company.get'] = 'password_change_required';
      await expectLater(connection.call({'method': 'company.get'}), throwsA(isA<ProtocolException>().having((e) => e.code, 'code', ErrorCode.passwordChangeRequired)));
      expect(connection.state.boardMemberStep, BoardMemberStep.changePassword);
    });

    test('a freeze placeholder changes nothing about the session', () async {
      await signedIn();
      daemon.notYet.add('session.list');
      await expectLater(connection.call({'method': 'session.list'}), throwsA(isA<ProtocolException>().having((e) => e.notOnServer, 'notOnServer', isTrue)));
      expect(connection.state.boardMemberStep, BoardMemberStep.ready);
    });

    test('MU3-04 AC1/AC4: sessions of me and of another board member, and revoke', () async {
      await signedIn();
      daemon.sessionFor('u-1', device: 'Desktop · dev-laptop');
      daemon.sessionFor('u-2', device: 'Android phone');
      final repository = AureliusRepository(connection);
      final mine = await repository.sessions();
      expect(mine.sessions.map((s) => (s.deviceName, s.current)), [('iPhone', true), ('Desktop · dev-laptop', false)]);
      final robins = await repository.sessions(userId: 'u-2');
      expect(robins.sessions.single.deviceName, 'Android phone');
      await repository.revokeSession(robins.sessions.single.id);
      expect(daemon.sessions.values.where((s) => s['user_id'] == 'u-2'), isEmpty);
    });

    test('MU3-05 AC4: the last active board member cannot be disabled', () async {
      await signedIn();
      final repository = AureliusRepository(connection);
      final disabled = await repository.disableBoardMember('u-2');
      expect(disabled.disabledAtMs, isNotNull);
      await expectLater(repository.disableBoardMember('u-1'), throwsA(isA<ProtocolException>().having((e) => e.code, 'code', ErrorCode.lastBoardMember)));
    });

    test('MU3-09 AC2 / B11: the hello brings the own Claude login; claude_login.updated asks again', () async {
      await signedIn();
      expect(connection.state.claudeLogin?.status, ClaudeLoginStatus.missing);
      daemon.claudeStatus = 'linked';
      daemon.event(daemon.current!, 'claude_login.updated');
      await until(() => connection.state.claudeLogin?.status == ClaudeLoginStatus.linked);
      expect(connection.state.claudeLogin?.email, 'test@example.com');
    });

    test('Sentinel C5–C8: the Connect code goes out once, in claude_login.input only, and is never logged or kept', () async {
      final logged = <String>[];
      final print = debugPrint;
      debugPrint = (message, {wrapWidth}) => logged.add('$message');
      addTearDown(() => debugPrint = print);
      await signedIn();
      final repository = AureliusRepository(connection);
      final prompts = <ClaudeLoginPrompt>[];
      final sub = repository.claudeLoginPrompts.listen(prompts.add);
      addTearDown(sub.cancel);
      final linkId = await repository.linkClaudeLogin();
      daemon.prompt(daemon.current!, linkId: linkId, url: 'https://claude.com/cai/oauth/authorize?code=true&state=x', asksCode: true);
      await until(() => prompts.isNotEmpty);
      expect(prompts.single.asksCode, isTrue);
      await repository.sendClaudeLoginCode(linkId, fakeCode);
      expect(daemon.claudeCodes, [fakeCode]);
      final carrying = daemon.frames.where((f) => f.contains(fakeCode)).toList();
      expect(carrying, hasLength(1));
      expect((jsonDecode(carrying.single) as Map)['method'], 'claude_login.input');
      expect((jsonDecode(carrying.single) as Map).containsKey('idempotency_key'), isFalse, reason: 'a session method: no receipt, no request hash');
      expect(logged.where((l) => l.contains(fakeCode)), isEmpty);
      expect(await store.sessionToken(testHost.hostId), isNot(contains(fakeCode)));
    });

    test('a freeze placeholder or an older server reads as "can\'t do this yet", not as an error of yours', () {
      const placeholder = ProtocolException(ErrorCode.validation, 'This server cannot do this yet.');
      const unknown = ProtocolException(ErrorCode.unknownMethod, 'This server does not know the method session.list.');
      const real = ProtocolException(ErrorCode.validation, 'A title is needed.');
      expect(placeholder.notOnServer, isTrue);
      expect(unknown.notOnServer, isTrue);
      expect(real.notOnServer, isFalse);
      expect(placeholder.shown, notOnServerMessage);
      expect(real.shown, 'A title is needed.');
    });

    test('MU3-08 AC1: the author comes from the session; a message carries no user of the phone', () async {
      await signedIn();
      await connection.write({'method': 'employee.send_message', 'params': {'employee_id': 'e1', 'text': 'Hello'}});
      final sent = jsonDecode(daemon.frames.lastWhere((f) => f.contains('employee.send_message'))) as Map<String, dynamic>;
      expect((sent['params'] as Map).keys, isNot(contains('user_id')));
      expect((sent['params'] as Map).keys, isNot(contains('author_user')));
    });

    test('QA3 M2: after a switch of board member on this phone the events come again (a new event.subscribe)', () async {
      await signedIn();
      expect(daemon.subscribes, 1);
      await connection.signOut();
      expect(daemon.current!.subscribed, isFalse, reason: 'the server stops the events with the board member');
      await connection.signIn('robin', robinPassword);
      expect(daemon.subscribes, 2, reason: 'Robin subscribes again on the same connection');
      final events = <Event>[];
      final sub = connection.events.listen(events.add);
      addTearDown(sub.cancel);
      daemon.event(daemon.current!, 'ticket.created');
      await until(() => events.isNotEmpty);
      expect(events.single.kind, EventKind.ticketCreated);
    });

    test('QA3 M1: Connect after a switch of board member: the link that follows the code reaches the phone', () async {
      await signedIn();
      await connection.signOut();
      await connection.signIn('robin', robinPassword);
      daemon.linksAfterInput = true;
      final repository = AureliusRepository(connection);
      final id = await repository.linkClaudeLogin();
      final answer = await repository.sendClaudeLoginCode(id, fakeCode);
      expect(answer.status, ClaudeLoginStatus.missing, reason: 'the server answers before Claude finished');
      await until(() => connection.state.claudeLogin?.status == ClaudeLoginStatus.linked);
      expect(connection.state.claudeLogin?.email, 'test@example.com');
    });

    test('MU3-03 AC1: a sign-in names this device for the sessions list', () async {
      connection.start();
      await until(() => connection.state.isOnline);
      await connection.signIn('robin', robinPassword);
      expect(daemon.sessions.values.single['device_name'], 'Test phone');
    });

    test('a server on the freeze gives an empty token: signed in on this connection, nothing kept', () async {
      daemon.emptyTokens = true;
      connection.start();
      await until(() => connection.state.isOnline);
      final login = await connection.signIn('alex', alexPassword);
      expect(login.sessionToken, isEmpty);
      expect(connection.state.user?.username, 'alex');
      expect(await store.sessionToken(testHost.hostId), isNull);
    });
  });

  group('S8: the session token lives in the Keychain / Keystore only', () {
    test('DeviceHostStore writes it to secure storage, never to SharedPreferences, and forgets Fase 1\'s id', () async {
      FlutterSecureStorage.setMockInitialValues({'board_member.host-1': 'u-1'});
      SharedPreferences.setMockInitialValues({});
      final store = DeviceHostStore();
      await store.save(testHost, 'device secret');
      await store.setSessionToken(testHost.hostId, 'aus_secret_token');
      expect(await store.sessionToken(testHost.hostId), 'aus_secret_token');
      const secure = FlutterSecureStorage();
      expect(await secure.read(key: 'board_member.host-1'), isNull, reason: 'Fase 1 remembered an id; protocol 5 signs in');
      final prefs = await SharedPreferences.getInstance();
      for (final key in prefs.getKeys()) {
        expect('${prefs.get(key)}', isNot(contains('aus_secret_token')), reason: key);
      }
      await store.setSessionToken(testHost.hostId, null);
      expect(await store.sessionToken(testHost.hostId), isNull);
      await store.setSessionToken(testHost.hostId, 'aus_again');
      await store.remove(testHost.hostId);
      expect(await secure.read(key: 'session_token.host-1'), isNull);
    });
  });

  group('screens', () {
    late MockRepository repo;
    late MockConnectionBloc link;
    late StreamController<LinkState> links;
    late StreamController<ClaudeLoginPrompt> prompts;

    void setLink(LinkState state) {
      when(() => link.state).thenReturn(state);
      when(() => repo.linkState).thenReturn(state);
      links.add(state);
    }

    setUp(() {
      repo = MockRepository();
      link = MockConnectionBloc();
      links = StreamController<LinkState>.broadcast();
      prompts = StreamController<ClaudeLoginPrompt>.broadcast();
      when(() => repo.host).thenReturn(testHost);
      when(() => repo.claudeLoginPrompts).thenAnswer((_) => prompts.stream);
      when(() => repo.claudeLogin()).thenAnswer((_) async => null);
      when(() => repo.dismissSignedOut()).thenReturn(null);
      when(() => repo.boardMemberChanges).thenAnswer((_) => const Stream.empty());
      whenListen(link, links.stream, initialState: alexIn);
    });

    tearDown(() {
      links.close();
      prompts.close();
    });

    Future<GoRouter> pump(WidgetTester tester, Widget page, {LinkState state = alexIn}) async {
      setLink(state);
      final router = GoRouter(initialLocation: '/', routes: [
        GoRoute(path: '/', builder: (_, _) => page),
        GoRoute(path: '/account/claude', builder: (_, _) => const Scaffold(body: Text('page claude'))),
        GoRoute(path: '/account', builder: (_, _) => const Scaffold(body: Text('page account'))),
        GoRoute(path: '/hosts', builder: (_, _) => const Scaffold(body: Text('page hosts'))),
        GoRoute(path: '/settings/board-members/:uid/sessions', builder: (_, s) => Scaffold(body: Text('sessions of ${s.uri.queryParameters['name']}'))),
        GoRoute(path: '/settings/board-members/:uid/password', builder: (_, s) => Scaffold(body: Text('password of ${s.uri.queryParameters['name']}'))),
      ]);
      addTearDown(router.dispose);
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: BlocProvider<ConnectionBloc>.value(value: link, child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router)),
      ));
      await tester.pumpAndSettle();
      return router;
    }

    Future<void> tapKey(WidgetTester tester, String key) async {
      await tester.ensureVisible(find.byKey(ValueKey(key)));
      await tester.tap(find.byKey(ValueKey(key)));
      await tester.pumpAndSettle();
    }

    /// The same, stepping time: the Connect sheet spins while Claude is asked.
    Future<void> tapStep(WidgetTester tester, String key) async {
      await tester.ensureVisible(find.byKey(ValueKey(key)));
      await tester.tap(find.byKey(ValueKey(key)));
      await settleBoard(tester);
    }

    testWidgets('MU3-02 AC1–AC3: Sign in, one generic error, never the password in a toast', (tester) async {
      await pump(tester, const SignInPage(), state: const LinkState(status: LinkStatus.online, boardMembersExist: true));
      expect(find.text('Sign in'), findsWidgets);
      expect(find.text('to my-pc (tailnet)'), findsOneWidget, reason: 'MU3-07: the server over the tailnet');
      expect(find.text('Forgot it? Ask another board member to reset your password.'), findsOneWidget);
      when(() => repo.signIn('alex', 'wrong one')).thenThrow(const ProtocolException(ErrorCode.wrongPassword, 'Username or password is wrong.'));
      await tester.enterText(find.byKey(const ValueKey('sign-in-username')), 'alex');
      await tester.enterText(find.byKey(const ValueKey('sign-in-password')), 'wrong one');
      await tapKey(tester, 'sign-in');
      expect(find.text(wrongSignIn), findsOneWidget);
      expect(find.descendant(of: find.byType(SnackBar), matching: find.textContaining('wrong one')), findsNothing);

      when(() => repo.signIn('alex', alexPassword)).thenAnswer((_) async {
        setLink(alexIn);
        return const LoginResult(expiresAtMs: 0, mustChangePassword: false, sessionToken: 'aus_1', user: alex);
      });
      await tester.enterText(find.byKey(const ValueKey('sign-in-password')), alexPassword);
      await tapKey(tester, 'sign-in');
      expect(find.text('Welcome, alex.'), findsOneWidget);
    });

    testWidgets('MU3-02 AC4 / B9: too_many_attempts counts down retry_after_ms with the button off', (tester) async {
      await pump(tester, const SignInPage(), state: const LinkState(status: LinkStatus.online, boardMembersExist: true));
      when(() => repo.signIn(any(), any())).thenThrow(const ProtocolException(ErrorCode.tooManyAttempts, 'Too many attempts.', retryAfterMs: 30000));
      await tester.enterText(find.byKey(const ValueKey('sign-in-username')), 'alex');
      await tester.enterText(find.byKey(const ValueKey('sign-in-password')), 'whatever it is');
      await tapKey(tester, 'sign-in');
      expect(find.text('Too many attempts. Try again in 0:30.'), findsOneWidget);
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('sign-in'))).onPressed, isNull);
      await tester.pump(const Duration(seconds: 10));
      expect(find.textContaining('Try again in 0:2'), findsWidgets);
      await tester.pump(const Duration(seconds: 21));
      expect(find.textContaining('Too many attempts'), findsNothing);
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('sign-in'))).onPressed, isNotNull);
    });

    testWidgets('ui §1: a session ended elsewhere shows "You were signed out on this device." once', (tester) async {
      await pump(tester, const SignInPage(), state: const LinkState(status: LinkStatus.online, boardMembersExist: true, signedOut: SignOutReason.revoked));
      expect(find.text('You were signed out on this device.'), findsOneWidget);
      expect(find.textContaining('Another device ended this session.'), findsOneWidget);
      await tester.tap(find.text('OK'));
      await tester.pumpAndSettle();
      verify(() => repo.dismissSignedOut()).called(1);
    });

    testWidgets('B10: the temporary password is changed first, with the temporary one as the old', (tester) async {
      await pump(tester, const ChangePasswordPage(required: true), state: const LinkState(status: LinkStatus.online, boardMembersExist: true, user: robin, mustChangePassword: true));
      expect(find.text('Choose a new password'), findsWidgets);
      when(() => repo.setPassword(newPassword: 'robin new long password', oldPassword: 'temporary password')).thenAnswer((_) async {});
      await tester.enterText(find.byKey(const ValueKey('password-old')), 'temporary password');
      await tester.enterText(find.byKey(const ValueKey('password-new')), 'robin new long password');
      await tester.enterText(find.byKey(const ValueKey('password-repeat')), 'robin new long password');
      await tester.pump();
      await tapKey(tester, 'password-save');
      verify(() => repo.setPassword(newPassword: 'robin new long password', oldPassword: 'temporary password')).called(1);
      expect(find.text('Password changed. Welcome.'), findsOneWidget);
    });

    test('B10 gate: with a temporary password only its change is reachable, not even New board member', () {
      expect(boardMemberRedirect(BoardMemberStep.changePassword, '/home'), '/onboarding');
      expect(boardMemberRedirect(BoardMemberStep.changePassword, '/onboarding/new'), '/onboarding');
      expect(boardMemberRedirect(BoardMemberStep.changePassword, '/account/sessions'), '/onboarding');
      expect(boardMemberRedirect(BoardMemberStep.changePassword, '/onboarding'), isNull);
    });

    testWidgets('MU3-04 AC3: More → Account → Sign out asks first, then signs out', (tester) async {
      when(() => repo.signOut()).thenAnswer((_) async {});
      await pump(tester, const AccountPage(), state: alexNoLogin);
      expect(find.text('alex'), findsOneWidget);
      expect(find.text('Not connected'), findsOneWidget, reason: 'the Claude login row says its status');
      await tapKey(tester, 'account-sign-out');
      expect(find.text('Sign out on this phone?'), findsOneWidget);
      await tester.tap(find.text('Sign out').last);
      await tester.pumpAndSettle();
      verify(() => repo.signOut()).called(1);
    });

    testWidgets('OnboardingPage follows the step: sign-in, or the change of a temporary password', (tester) async {
      await pump(tester, const OnboardingPage(), state: const LinkState(status: LinkStatus.online, boardMembersExist: true));
      expect(find.byKey(const ValueKey('sign-in')), findsOneWidget);
      setLink(const LinkState(status: LinkStatus.online, boardMembersExist: true, user: robin, mustChangePassword: true));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('password-save')), findsOneWidget);
      expect(find.byKey(const ValueKey('password-sign-out')), findsOneWidget);
    });

    testWidgets('MU3-04: Sessions marks this device, Revoke asks first and then ends the other', (tester) async {
      final now = DateTime.now().millisecondsSinceEpoch;
      when(() => repo.sessions(userId: any(named: 'userId'))).thenAnswer((_) async => SessionsSnapshot(userId: 'u-1', sessions: [
            SessionView(id: 's-1', userId: 'u-1', deviceName: 'iPhone', current: true, createdAtMs: now, lastSeenMs: now, expiresAtMs: 0),
            SessionView(id: 's-2', userId: 'u-1', deviceName: 'Desktop · dev-laptop', current: false, createdAtMs: now, lastSeenMs: now - const Duration(hours: 2).inMilliseconds, expiresAtMs: 0, clientId: 'c-1'),
          ]));
      when(() => repo.revokeSession('s-2')).thenAnswer((_) async {});
      await pump(tester, const SessionsPage());
      expect(find.text('Devices signed in as alex'), findsOneWidget);
      expect(find.text('This device'), findsOneWidget);
      expect(find.byKey(const ValueKey('revoke-s-1')), findsNothing, reason: 'this phone signs out with Sign out');
      expect(find.textContaining('active 2 h ago'), findsOneWidget);
      await tapKey(tester, 'revoke-s-2');
      expect(find.text('Sign out Desktop · dev-laptop?'), findsOneWidget);
      expect(find.text('It needs your password to come back.'), findsOneWidget);
      await tester.tap(find.text('Revoke').last);
      await tester.pumpAndSettle();
      verify(() => repo.revokeSession('s-2')).called(1);
      expect(find.text('Desktop · dev-laptop'), findsNothing);
    });

    testWidgets('a server without session.list yet says so, with Try again', (tester) async {
      when(() => repo.sessions(userId: any(named: 'userId'))).thenThrow(const ProtocolException(ErrorCode.validation, 'This server cannot do this yet.'));
      await pump(tester, const SessionsPage());
      expect(find.text(notOnServerMessage), findsOneWidget);
      expect(find.text('Try again'), findsOneWidget);
    });

    testWidgets('MU3-05 AC3/AC4: Disable asks first; the last active board member is refused in words', (tester) async {
      when(() => repo.boardMembers()).thenAnswer((_) async => [alex, robin]);
      when(() => repo.disableBoardMember('u-2')).thenThrow(const ProtocolException(ErrorCode.lastBoardMember, ''));
      await pump(tester, const BoardMembersPage());
      await tapKey(tester, 'board-members-u-2');
      expect(find.text('Sessions'), findsOneWidget);
      expect(find.text('Set a temporary password'), findsOneWidget);
      await tapKey(tester, 'member-disable');
      expect(find.textContaining('signed out on every device'), findsOneWidget);
      await tester.tap(find.text('Disable'));
      await tester.pumpAndSettle();
      expect(find.text('The last active board member cannot be disabled.'), findsOneWidget);
    });

    testWidgets('B10 / P-34: another board member gets a temporary password from the members list', (tester) async {
      when(() => repo.boardMembers()).thenAnswer((_) async => [alex, robin]);
      await pump(tester, const BoardMembersPage());
      await tapKey(tester, 'board-members-u-2');
      await tapKey(tester, 'member-reset');
      expect(find.text('password of robin'), findsOneWidget);
    });

    testWidgets('B10: the temporary password goes out with the user id, and says what happens', (tester) async {
      when(() => repo.setPassword(newPassword: 'temporary password', userId: 'u-2')).thenAnswer((_) async {});
      await pump(tester, const TemporaryPasswordPage(userId: 'u-2', username: 'robin'));
      expect(find.textContaining('All their devices are signed out now.'), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('temporary-password')), 'temporary password');
      await tester.pump();
      await tapKey(tester, 'temporary-password-save');
      verify(() => repo.setPassword(newPassword: 'temporary password', userId: 'u-2')).called(1);
    });

    testWidgets('MU3-09 AC1 / B1-3 / F2: not connected: "Connect your own Claude account (at Anthropic)" and the residual risk', (tester) async {
      await pump(tester, const ClaudeLoginPage(), state: alexNoLogin);
      expect(find.text('Not connected. Your tickets wait until you connect.'), findsOneWidget);
      expect(find.text('Connect your own Claude account (at Anthropic)'), findsOneWidget);
      expect(find.byKey(const ValueKey('claude-risk')), findsOneWidget);
      expect(find.text(claudeLoginRisk), findsOneWidget);
      expect(find.textContaining('Sign in with Claude'), findsNothing, reason: 'B1-3: never an Aurelius sign-in with Claude');
    });

    testWidgets('MU3-09 AC2/AC4: connected shows email and plan; Disconnect asks first', (tester) async {
      when(() => repo.unlinkClaudeLogin()).thenAnswer((_) async {});
      await pump(tester, const ClaudeLoginPage());
      expect(find.text('Connected'), findsOneWidget);
      expect(find.text('test@example.com · Max'), findsOneWidget);
      await tapKey(tester, 'claude-disconnect');
      expect(find.text('Disconnect your Claude login?'), findsOneWidget);
      await tester.tap(find.text('Disconnect').last);
      await tester.pumpAndSettle();
      verify(() => repo.unlinkClaudeLogin()).called(1);
    });

    SubscriptionUsage usageOf(List<PlanLimitView> limits, {String raw = '', String? error}) =>
        SubscriptionUsage(fetchedAtMs: 1, limits: limits, raw: raw, error: error, loginUser: 'u-1', plan: 'max');

    testWidgets('MU3-09 AC2 / ui §4: the own /usage as bars with their reset; Check now asks it fresh', (tester) async {
      when(() => repo.claudeLogin()).thenAnswer((_) async => alexIn.claudeLogin);
      when(() => repo.subscription(refresh: any(named: 'refresh'))).thenAnswer((_) async => usageOf(const [
            PlanLimitView(label: 'Current session', percent: 12, resets: '4pm (UTC)'),
            PlanLimitView(label: 'Current week (all models)', percent: 34, resets: 'Mon 8am (UTC)'),
          ]));
      await pump(tester, const ClaudeLoginPage());
      expect(find.byKey(const ValueKey('claude-usage')), findsOneWidget);
      expect(find.text('Current session'), findsOneWidget);
      expect(find.text('12 % · resets 4pm (UTC)'), findsOneWidget);
      expect(find.text('34 % · resets Mon 8am (UTC)'), findsOneWidget);
      expect(find.byType(LinearProgressIndicator), findsNWidgets(2));
      verify(() => repo.subscription(refresh: false)).called(1);
      await tapKey(tester, 'claude-check');
      verify(() => repo.subscription(refresh: true)).called(1);
    });

    testWidgets('ui §4: a full limit says "Limit reached until …" with the bar at 100 %, without an action', (tester) async {
      when(() => repo.claudeLogin()).thenAnswer((_) async => alexIn.claudeLogin);
      when(() => repo.subscription(refresh: any(named: 'refresh'))).thenAnswer((_) async => usageOf(const [
            PlanLimitView(label: 'Current session', percent: 100, resets: '16:00'),
          ]));
      await pump(tester, const ClaudeLoginPage());
      expect(find.text('Limit reached until 16:00.'), findsOneWidget);
      expect(find.text('100 % · resets 16:00'), findsOneWidget);
      expect(tester.widget<LinearProgressIndicator>(find.byType(LinearProgressIndicator)).value, 1.0);
    });

    testWidgets('MU3-09 AC5 / ui §4: an API login without "% used" is "No subscription", never 0 %', (tester) async {
      when(() => repo.claudeLogin()).thenAnswer((_) async => alexIn.claudeLogin);
      when(() => repo.subscription(refresh: any(named: 'refresh')))
          .thenAnswer((_) async => usageOf(const [], raw: 'You are using an API key; there are no plan limits to show.'));
      await pump(tester, const ClaudeLoginPage());
      expect(find.text(claudeNoSubscription), findsOneWidget);
      expect(find.textContaining('0 %'), findsNothing);
      expect(find.byKey(const ValueKey('claude-reconnect')), findsOneWidget);
    });

    testWidgets('an empty first /usage (the server still fetches it) is asked once more', (tester) async {
      final retry = usageRetry;
      usageRetry = const Duration(milliseconds: 10);
      addTearDown(() => usageRetry = retry);
      var calls = 0;
      when(() => repo.claudeLogin()).thenAnswer((_) async => alexIn.claudeLogin);
      when(() => repo.subscription(refresh: any(named: 'refresh'))).thenAnswer((_) async => ++calls == 1
          ? const SubscriptionUsage(fetchedAtMs: 0, limits: [], raw: '')
          : usageOf(const [PlanLimitView(label: 'Current session', percent: 40)]));
      await pump(tester, const ClaudeLoginPage());
      await tester.pump(const Duration(milliseconds: 20));
      await tester.pumpAndSettle();
      expect(calls, 2);
      expect(find.text('40 %'), findsOneWidget);
    });

    testWidgets('not connected: no /usage is asked', (tester) async {
      when(() => repo.claudeLogin()).thenAnswer((_) async => alexNoLogin.claudeLogin);
      await pump(tester, const ClaudeLoginPage(), state: alexNoLogin);
      verifyNever(() => repo.subscription(refresh: any(named: 'refresh')));
      expect(find.byKey(const ValueKey('claude-usage')), findsNothing);
    });

    testWidgets('M2 smoke run: connected while the page is open, its /usage follows', (tester) async {
      when(() => repo.claudeLogin()).thenAnswer((_) async => alexNoLogin.claudeLogin);
      when(() => repo.subscription(refresh: any(named: 'refresh')))
          .thenAnswer((_) async => usageOf(const [PlanLimitView(label: 'Session (5 hours)', percent: 12, resets: '4pm (UTC)')]));
      await pump(tester, const ClaudeLoginPage(), state: alexNoLogin);
      verifyNever(() => repo.subscription(refresh: any(named: 'refresh')));
      setLink(alexIn);
      await tester.pumpAndSettle();
      expect(find.text('12 % · resets 4pm (UTC)'), findsOneWidget);
    });

    test('D5 and B1-2: the texts of the server team, word for word', () {
      expect(claudeLoginRisk,
          "Your Claude login is kept on this server. Other board members' agents could read your Claude login's files: their file tools are blocked from it, but a command they run is not. Connect only if you trust the work that runs on this server.");
      expect(ownSubscriptionNote, 'Tasks of this app run on your own Claude subscription.');
      expect(claudeRevokeAtAnthropic, 'To end it at Anthropic too, revoke the session at claude.ai → Settings.');
      expect(claudeUsageState(null), ClaudeUsageState.none);
      expect(claudeUsageState(const SubscriptionUsage(fetchedAtMs: 5, limits: [], raw: '', error: 'Connect your Claude login to see your usage.')), ClaudeUsageState.error);
    });

    testWidgets('P-32 / C3 / C7: Connect opens Anthropic in the system browser, sends the pasted code once, never shows it', (tester) async {
      final opened = <Uri>[];
      final open = openClaudeSignIn;
      openClaudeSignIn = (uri) async {
        opened.add(uri);
        return true;
      };
      addTearDown(() => openClaudeSignIn = open);
      when(() => repo.linkClaudeLogin()).thenAnswer((_) async => 'link-1');
      when(() => repo.sendClaudeLoginCode('link-1', fakeCode)).thenAnswer((_) async => const ClaudeLoginView(userId: 'u-1', status: ClaudeLoginStatus.linked, background: false));
      when(() => repo.applyClaudeLogin(any())).thenReturn(null);
      when(() => repo.cancelClaudeLogin(any())).thenAnswer((_) async {});
      await pump(tester, const ClaudeLoginPage(), state: alexNoLogin);
      await tapStep(tester, 'claude-connect');
      expect(find.text('Connect your Claude login'), findsOneWidget);
      expect(find.text('Aurelius never sees your Claude password.'), findsOneWidget);
      expect(find.byKey(const ValueKey('connect-risk')), findsOneWidget);

      prompts.add(const ClaudeLoginPrompt(linkId: 'link-1', url: 'https://claude.com/cai/oauth/authorize?code=true&state=x', asksCode: true));
      await settleBoard(tester);
      await tapStep(tester, 'connect-open');
      expect(opened.single.host, 'claude.com');

      await tester.enterText(find.byKey(const ValueKey('connect-code')), 'not a code\nwith a line');
      await tester.pump();
      expect(tester.widget<FilledButton>(find.byKey(const ValueKey('connect-send'))).onPressed, isNull, reason: 'C7: only the code shape');

      await tester.enterText(find.byKey(const ValueKey('connect-code')), fakeCode);
      await tester.pump();
      await tapStep(tester, 'connect-send');
      verify(() => repo.sendClaudeLoginCode('link-1', fakeCode)).called(1);
      expect(find.text('Your Claude login is connected.'), findsOneWidget);
      expect(find.textContaining(fakeCode), findsNothing);
      verifyNever(() => repo.cancelClaudeLogin(any()));
    });

    testWidgets('QA3 M1: a good code the server confirms later closes the sheet with success; page and Account row say Connected', (tester) async {
      final every = connectCheckEvery;
      connectCheckEvery = const Duration(milliseconds: 50);
      addTearDown(() => connectCheckEvery = every);
      var linked = false;
      when(() => repo.linkClaudeLogin()).thenAnswer((_) async => 'link-1');
      // As the real server: the answer to the code comes before Claude finished.
      when(() => repo.sendClaudeLoginCode('link-1', fakeCode)).thenAnswer((_) async {
        linked = true;
        return const ClaudeLoginView(userId: 'u-1', status: ClaudeLoginStatus.missing, background: false, linkId: 'link-1');
      });
      when(() => repo.applyClaudeLogin(any())).thenReturn(null);
      when(() => repo.cancelClaudeLogin(any())).thenAnswer((_) async {});
      when(() => repo.subscription(refresh: any(named: 'refresh'))).thenAnswer((_) async => usageOf(const [PlanLimitView(label: 'Current session', percent: 12)]));
      // No claude_login.updated arrives: only asking the status shows the link.
      when(() => repo.claudeLogin()).thenAnswer((_) async {
        if (!linked) return alexNoLogin.claudeLogin;
        setLink(alexIn);
        return alexIn.claudeLogin;
      });
      await tester.binding.setSurfaceSize(const Size(900, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await pump(tester, const Row(children: [Expanded(child: AccountPage()), Expanded(child: ClaudeLoginPage())]), state: alexNoLogin);
      expect(find.text('Not connected'), findsOneWidget, reason: 'the Account row');
      await tapStep(tester, 'claude-connect');
      prompts.add(const ClaudeLoginPrompt(linkId: 'link-1', url: 'https://claude.com/cai/oauth/authorize?code=true&state=x', asksCode: true));
      await settleBoard(tester);
      await tester.enterText(find.byKey(const ValueKey('connect-code')), fakeCode);
      await tester.pump();
      await tapStep(tester, 'connect-send');
      await tester.pump(const Duration(milliseconds: 60));
      await settleBoard(tester);
      expect(find.byType(ConnectSheet), findsNothing, reason: 'the sheet closed');
      expect(find.text('Your Claude login is connected.'), findsWidgets, reason: 'a snackbar on each of the two scaffolds');
      expect(find.text('Connected'), findsNWidgets(2), reason: 'the page and the Account row, without opening them again');
      expect(find.text('Checking the code with Claude…'), findsNothing);
      verifyNever(() => repo.cancelClaudeLogin(any()));
    });

    testWidgets('QA3 M-m1: a connected login whose /usage was not read yet is read fresh, else "Usage not checked yet."', (tester) async {
      when(() => repo.claudeLogin()).thenAnswer((_) async => alexIn.claudeLogin);
      // Kept from before the connect: the server's "connect first".
      when(() => repo.subscription(refresh: false)).thenAnswer((_) async => const SubscriptionUsage(fetchedAtMs: 5, limits: [], raw: '', error: 'Connect your Claude login to see your usage.', loginUser: 'u-1'));
      when(() => repo.subscription(refresh: true)).thenAnswer((_) async => usageOf(const [PlanLimitView(label: 'Current session', percent: 21)]));
      await pump(tester, const ClaudeLoginPage());
      verify(() => repo.subscription(refresh: true)).called(1);
      expect(find.text('21 %'), findsOneWidget);
      expect(find.textContaining('Connect your Claude login to see'), findsNothing);
    });

    testWidgets('QA3 M-m1: a /usage still unread after the fresh ask says "Usage not checked yet.", not the Connect text', (tester) async {
      when(() => repo.claudeLogin()).thenAnswer((_) async => alexIn.claudeLogin);
      when(() => repo.subscription(refresh: any(named: 'refresh'))).thenAnswer((_) async => const SubscriptionUsage(fetchedAtMs: 5, limits: [], raw: '', error: 'Connect your Claude login to see your usage.', loginUser: 'u-1'));
      await pump(tester, const ClaudeLoginPage());
      expect(find.text(claudeUsageNotChecked), findsOneWidget);
      expect(find.textContaining('Connect your Claude login to see'), findsNothing);
    });

    testWidgets('R3 m3: a /usage of another login is never shown', (tester) async {
      when(() => repo.claudeLogin()).thenAnswer((_) async => alexIn.claudeLogin);
      when(() => repo.subscription(refresh: any(named: 'refresh'))).thenAnswer((_) async =>
          const SubscriptionUsage(fetchedAtMs: 1, limits: [PlanLimitView(label: 'Current session', percent: 77)], raw: '', loginUser: 'u-2', plan: 'max'));
      await pump(tester, const ClaudeLoginPage());
      expect(find.text('77 %'), findsNothing);
      expect(find.text(claudeUsageNotChecked), findsOneWidget);
      expect(ownUsage(usageOf(const []), 'u-1'), isNotNull);
      expect(ownUsage(usageOf(const []), 'u-2'), isNull);
    });

    testWidgets('SEC2 m4: Disconnect says how to end the session at Anthropic too', (tester) async {
      when(() => repo.unlinkClaudeLogin()).thenAnswer((_) async => setLink(alexNoLogin));
      when(() => repo.claudeLogin()).thenAnswer((_) async => repo.linkState.claudeLogin);
      when(() => repo.subscription(refresh: any(named: 'refresh'))).thenAnswer((_) async => usageOf(const []));
      await pump(tester, const ClaudeLoginPage());
      await tapKey(tester, 'claude-disconnect');
      expect(find.textContaining(claudeRevokeAtAnthropic), findsOneWidget, reason: 'in the question');
      await tester.tap(find.text('Disconnect').last);
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('claude-revoke-note')), findsOneWidget);
      expect(find.textContaining(claudeRevokeAtAnthropic), findsNWidgets(2), reason: 'the note and the toast');
    });

    test('R3 m5: a board member whose login was not read yet counts as not connected', () {
      expect(claudeLoginMissing(const LinkState(status: LinkStatus.online, boardMembersExist: true, user: alex)), isTrue);
      expect(claudeLoginMissing(const LinkState(status: LinkStatus.online, boardMembersExist: true)), isFalse, reason: 'no board member: no login to ask for');
      expect(claudeLoginMissing(alexIn), isFalse);
    });

    testWidgets('Connect: Claude\'s own failure text with Try again; Cancel stops the link', (tester) async {
      when(() => repo.linkClaudeLogin()).thenAnswer((_) async => 'link-1');
      when(() => repo.cancelClaudeLogin('link-1')).thenAnswer((_) async {});
      await pump(tester, const ClaudeLoginPage(), state: alexNoLogin);
      await tapStep(tester, 'claude-connect');
      prompts.add(const ClaudeLoginPrompt(linkId: 'link-1', asksCode: false, error: 'De code werd niet aanvaard. Start opnieuw.'));
      await settleBoard(tester);
      expect(find.text('De code werd niet aanvaard. Start opnieuw.'), findsOneWidget);
      expect(find.byKey(const ValueKey('connect-again')), findsOneWidget);
      await tapStep(tester, 'connect-again');
      verify(() => repo.linkClaudeLogin()).called(2);
      await tapStep(tester, 'connect-cancel');
      verify(() => repo.cancelClaudeLogin('link-1')).called(1);
    });

    testWidgets('P-32 fallback / B1-4: a server without the guided flow offers the terminal command', (tester) async {
      when(() => repo.linkClaudeLogin()).thenThrow(const ProtocolException(ErrorCode.validation, 'This server cannot do this yet.'));
      await pump(tester, const ClaudeLoginPage(), state: alexNoLogin);
      await tapStep(tester, 'claude-connect');
      expect(find.text(notOnServerMessage), findsOneWidget);
      expect(find.text(claudeLoginCommand('u-1')), findsOneWidget);
      expect(find.textContaining('--console or --sso'), findsOneWidget);
    });

    testWidgets('ui §4: Home reminds you to connect your Claude login', (tester) async {
      await pump(tester, const Scaffold(body: ClaudeLoginCard()), state: alexNoLogin);
      expect(find.text('Connect your Claude login to run your tickets.'), findsOneWidget);
      await tester.tap(find.text('Connect'));
      await tester.pumpAndSettle();
      expect(find.text('page claude'), findsOneWidget);
    });

    testWidgets('a connected login shows no reminder', (tester) async {
      await pump(tester, const Scaffold(body: ClaudeLoginCard()));
      expect(find.byKey(const ValueKey('claude-login-card')), findsNothing);
    });
  });

  group('tickets wait for a login or a limit (MU3-10..12, B4, P-37)', () {
    // The board harness signs in as its testUser; Robin is the other board member.
    const alexUser = testUser;
    const robinUser = UserView(id: 'u-2', username: 'Robin', createdAtMs: 1);

    test('MU3-12: needs login is yours (Needs you) or another\'s (no Needs you)', () {
      final mine = ticketOf(column: ColumnKind.doing, needs: TicketNeed.login);
      final theirs = ticketOf(column: ColumnKind.doing, creator: robinUser, needs: TicketNeed.login);
      expect(ticketState(mine, me: alexUser.id), TicketState.needsYourLogin);
      expect(ticketState(mine, me: alexUser.id).needsYou, isTrue);
      expect(ticketState(theirs, me: alexUser.id), TicketState.waitsForLogin);
      expect(ticketState(theirs, me: alexUser.id).needsYou, isFalse, reason: 'ui §3: a login wait counts for its creator only');
      expect(TicketState.needsYourLogin.line(expired: true), 'Waiting for your Claude login (expired)');
      expect(TicketState.waitsForLogin.line(login: 'Robin'), "Waiting for Robin's Claude login");
      expect(ticketState(ticketOf(column: ColumnKind.backlog, needs: TicketNeed.login), me: alexUser.id), TicketState.backlog);
    });

    test('MU3-11 AC2 / P-37: a parked main task reads "Waiting for Robin\'s limit until 16:00", only the name and the time', () {
      final at = DateTime(2026, 11, 3, 16);
      final root = taskOf(id: 'root', status: TaskStatus.waiting, waitingReason: 'quota', waitingUntilMs: at.millisecondsSinceEpoch);
      final ticket = ticketOf(column: ColumnKind.doing, creator: robinUser, rootTaskId: 'root');
      expect(ticketState(ticket, root: root, me: alexUser.id), TicketState.waitingForLimit);
      expect(limitWaitOf("Robin's", at.millisecondsSinceEpoch, now: DateTime(2026, 11, 3, 9)), "Waiting for Robin's limit until 16:00");
      expect(limitWaitOf('your', at.millisecondsSinceEpoch, now: DateTime(2026, 11, 3, 9)), 'Waiting for your limit until 16:00');
      expect(limitWaitOf("Robin's", null), "Waiting for Robin's limit to reset");
      expect(limitWaitOf("Robin's", at.millisecondsSinceEpoch, now: DateTime(2026, 11, 2, 9)), "Waiting for Robin's limit until Tue 3 Nov 16:00");
    });

    TicketDetail detailOf(TicketView ticket, {TaskView? root}) => TicketDetail(asOfSeq: 1, ticket: ticket, rootTask: root, tasks: [?root]);

    Future<void> tapKey(WidgetTester tester, String key) async {
      await tester.ensureVisible(find.byKey(ValueKey(key)));
      await tester.tap(find.byKey(ValueKey(key)));
      await settleBoard(tester);
    }

    testWidgets('B4: a ticket waiting for Robin\'s login offers Run on my login; it sends ticket.use_my_login with the ticket only', (tester) async {
      final ticket = ticketOf(id: 'k1', number: 42, column: ColumnKind.doing, creator: robinUser, needs: TicketNeed.login);
      final repo = boardRepository(detail: detailOf(ticket));
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: 'k1'));
      expect(find.text("⏸ Waiting for Robin's Claude login"), findsOneWidget);
      expect(find.text("Robin's Claude login"), findsOneWidget, reason: 'MU3-10 AC4: whose subscription it runs on');
      await tapKey(tester, 'bar-use-my-login');
      expect(find.text('Run #42 on your Claude login?'), findsOneWidget);
      await tester.tap(find.text('Run on my login').last);
      await settleBoard(tester);
      final sent = verify(() => repo.write(captureAny(), expectedVersion: 3, retry: false, idempotencyKey: any(named: 'idempotencyKey'))).captured.single as Map<String, dynamic>;
      expect(sent['method'], 'ticket.use_my_login');
      expect(sent['params'], {'ticket_id': 'k1'}, reason: 'B1-1: never another board member\'s login');
      expect(find.text('#42 runs on your Claude login now.'), findsOneWidget);
    });

    testWidgets('MU3-12 AC2: my own ticket without a login is a Needs you with Connect', (tester) async {
      final ticket = ticketOf(id: 'k1', column: ColumnKind.doing, needs: TicketNeed.login);
      final repo = boardRepository(detail: detailOf(ticket));
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: 'k1'));
      expect(find.text('◆ Waiting for your Claude login'), findsWidgets);
      expect(find.text('Your Claude login'), findsOneWidget);
      expect(find.byKey(const ValueKey('bar-connect')), findsOneWidget);
      expect(find.byKey(const ValueKey('bar-use-my-login')), findsNothing, reason: 'nothing to take over from yourself');
    });

    testWidgets('MU3-11: a ticket parked on Robin\'s limit shows the time and no action', (tester) async {
      final until = DateTime.now().add(const Duration(minutes: 5));
      final root = taskOf(id: 'root', status: TaskStatus.waiting, waitingReason: 'quota', waitingUntilMs: until.millisecondsSinceEpoch, ticketId: 'k1');
      final ticket = ticketOf(id: 'k1', column: ColumnKind.doing, creator: robinUser, rootTaskId: 'root', rootTaskStatus: TaskStatus.waiting);
      final repo = boardRepository(detail: detailOf(ticket, root: root));
      await pumpBoardPage(tester, repo, const TicketPage(ticketId: 'k1'));
      expect(find.textContaining("Waiting for Robin's limit until"), findsWidgets);
      expect(find.byKey(const ValueKey('bar-use-my-login')), findsNothing);
      expect(find.byKey(const ValueKey('bar-connect')), findsNothing);
    });
  });
}
