import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/more/mcp_login_sheet.dart';
import 'package:aurelius_mobile/features/more/mcp_section.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

class MockRepository extends Mock implements AureliusRepository {}

McpServerInfo server(String name, {McpAuthStatus? auth, McpTransport transport = McpTransport.http}) =>
    McpServerInfo(name: name, scope: McpScope.user, transport: transport, summary: 'https://$name.example/mcp', enabled: true, auth: auth);

McpLogin loginOf(McpLoginState state, {String? error}) => McpLogin(
      loginId: 'l1',
      name: 'linear',
      state: state,
      authorizationUrl: 'https://auth.example/authorize?x=1',
      redirectUri: 'http://localhost:51234/callback',
      expiresAtMs: 0,
      error: error,
    );

void main() {
  test('McpServers parses the list and keeps missing names enabled', () {
    final s = McpServers.fromJson({
      'servers': [
        {'name': 'a', 'scope': 'user', 'transport': 'stdio', 'summary': 'npx a', 'enabled': true},
        {'name': 'b', 'scope': 'user', 'transport': 'http', 'summary': 'https://x', 'enabled': false, 'auth': 'needs_login'},
      ],
      'missing': ['gone'],
    });
    expect(s.servers.length, 2);
    expect(s.enabledNames, ['a', 'gone']);
    expect(s.servers.first.auth, isNull, reason: 'older servers leave auth out');
    expect(s.servers.last.auth, McpAuthStatus.needsLogin);
  });

  test('an empty list enables nothing', () {
    const s = McpServers(servers: [], missing: []);
    expect(s.enabledNames, isEmpty);
  });

  test('auth badges and actions', () {
    expect(server('a', auth: McpAuthStatus.authorized).authLabel, 'Connected');
    expect(server('a', auth: McpAuthStatus.needsLogin).authLabel, 'Needs login');
    expect(server('a', auth: McpAuthStatus.unknown).authLabel, 'Unknown');
    expect(server('a', auth: McpAuthStatus.none).authLabel, isNull);
    expect(server('a').authLabel, isNull);
    expect((server('a', auth: McpAuthStatus.authorized).canLogOut, server('a', auth: McpAuthStatus.authorized).canLogIn), (true, false));
    expect(server('a', auth: McpAuthStatus.unknown).canLogIn, isTrue);
  });

  test('the callback address is recognised', () {
    const redirect = 'http://localhost:51234/callback';
    expect(looksLikeMcpCallback('http://localhost:51234/callback?code=abc&state=xyz', redirectUri: redirect), isTrue);
    expect(looksLikeMcpCallback('  http://127.0.0.1:9/callback?code=abc  '), isTrue);
    expect(looksLikeMcpCallback('http://localhost:51234/callback?error=access_denied&state=x'), isTrue);
    expect(looksLikeMcpCallback('http://localhost:51234/callback'), isFalse, reason: 'no code');
    expect(looksLikeMcpCallback('https://auth.example/authorize?code=1'), isFalse);
    expect(looksLikeMcpCallback('hello'), isFalse);
  });

  test('login problems', () {
    expect(mcpLoginProblem(loginOf(McpLoginState.failed, error: 'denied')), 'denied');
    expect(mcpLoginProblem(loginOf(McpLoginState.expired)), contains('expired'));
    expect(mcpLoginProblem(loginOf(McpLoginState.pending)), isNull);
  });

  group('McpSection', () {
    late MockRepository repo;
    late StreamController<Event> events;
    final opened = <Uri>[];
    String? clipboard;

    setUp(() {
      repo = MockRepository();
      events = StreamController<Event>.broadcast();
      opened.clear();
      clipboard = null;
      openMcpLoginPage = (uri) async {
        opened.add(uri);
        return true;
      };
      when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.online, capabilities: ['approve']));
      when(() => repo.mcpAuthChanges).thenAnswer((_) => events.stream);
    });

    tearDown(() => events.close());

    Future<void> pump(WidgetTester tester) async {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
        if (call.method == 'Clipboard.getData') return clipboard == null ? null : {'text': clipboard};
        if (call.method == 'Clipboard.setData') clipboard = (call.arguments as Map)['text'] as String?;
        return null;
      });
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: RepositoryProvider<AureliusRepository>.value(value: repo, child: const SingleChildScrollView(child: McpSection())),
        ),
      ));
      await tester.pumpAndSettle();
    }

    testWidgets('shows a badge per remote server and nothing on older servers', (tester) async {
      when(() => repo.mcpServers()).thenAnswer((_) async => McpServers(servers: [
            server('linear', auth: McpAuthStatus.authorized),
            server('notion', auth: McpAuthStatus.needsLogin),
            server('fs', auth: McpAuthStatus.none, transport: McpTransport.stdio),
            server('old'),
          ], missing: const []));
      await pump(tester);
      expect(find.text('Connected'), findsOneWidget);
      expect(find.text('Needs login'), findsOneWidget);
      expect(find.byKey(const ValueKey('mcp-logout-linear')), findsOneWidget);
      expect(find.byKey(const ValueKey('mcp-login-notion')), findsOneWidget);
      expect(find.byKey(const ValueKey('mcp-auth-fs')), findsNothing);
      expect(find.byKey(const ValueKey('mcp-auth-old')), findsNothing);
    });

    testWidgets('log in opens the browser and the pasted address finishes it', (tester) async {
      var auth = McpAuthStatus.needsLogin;
      when(() => repo.mcpServers()).thenAnswer((_) async => McpServers(servers: [server('linear', auth: auth)], missing: const []));
      when(() => repo.startMcpLogin('linear')).thenAnswer((_) async => loginOf(McpLoginState.pending));
      const address = 'http://localhost:51234/callback?code=abc&state=xyz';
      when(() => repo.completeMcpLogin('l1', address)).thenAnswer((_) async {
        auth = McpAuthStatus.authorized;
        return loginOf(McpLoginState.succeeded);
      });
      clipboard = address;
      await pump(tester);

      await tester.tap(find.byKey(const ValueKey('mcp-login-linear')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('mcp-login-sheet')), findsOneWidget);
      expect(opened.single.toString(), 'https://auth.example/authorize?x=1');
      expect(find.text(address), findsOneWidget, reason: 'prefilled from the clipboard');

      await tester.tap(find.byKey(const ValueKey('mcp-login-finish')));
      await tester.pumpAndSettle();
      verify(() => repo.completeMcpLogin('l1', address)).called(1);
      expect(find.byKey(const ValueKey('mcp-login-sheet')), findsNothing);
      expect(find.text('Connected'), findsOneWidget);
    });

    testWidgets('a refused address keeps the login waiting and shows why', (tester) async {
      when(() => repo.mcpServers()).thenAnswer((_) async => McpServers(servers: [server('linear', auth: McpAuthStatus.unknown)], missing: const []));
      when(() => repo.startMcpLogin('linear')).thenAnswer((_) async => loginOf(McpLoginState.pending));
      when(() => repo.completeMcpLogin('l1', any())).thenThrow(const ProtocolException(ErrorCode.invalidRequest, 'That address has no code.'));
      await pump(tester);

      await tester.tap(find.byKey(const ValueKey('mcp-login-linear')));
      await tester.pumpAndSettle();
      expect(find.text('Unknown'), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('mcp-login-address')), 'http://localhost:1/callback');
      await tester.tap(find.byKey(const ValueKey('mcp-login-finish')));
      await tester.pumpAndSettle();
      expect(find.text('That address has no code.'), findsOneWidget);
      expect(find.byKey(const ValueKey('mcp-login-sheet')), findsOneWidget);

      await tester.tap(find.text('Open again'));
      await tester.pumpAndSettle();
      expect(opened, hasLength(2));
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('mcp-login-sheet')), findsNothing);
    });

    testWidgets('a login that ends elsewhere closes the sheet', (tester) async {
      when(() => repo.mcpServers()).thenAnswer((_) async => McpServers(servers: [server('linear', auth: McpAuthStatus.needsLogin)], missing: const []));
      when(() => repo.startMcpLogin('linear')).thenAnswer((_) async => loginOf(McpLoginState.pending));
      when(() => repo.mcpLogin('l1')).thenAnswer((_) async => loginOf(McpLoginState.failed, error: 'access_denied'));
      await pump(tester);
      await tester.tap(find.byKey(const ValueKey('mcp-login-linear')));
      await tester.pumpAndSettle();
      events.add(const Event(aggregateId: 'l1', atMs: 1, kind: EventKind.mcpLoginFinished, seq: 1));
      await tester.pumpAndSettle();
      expect(find.text('access_denied'), findsOneWidget);
      expect(find.text('Close'), findsOneWidget);
    });

    testWidgets('log out asks first and shows the new list', (tester) async {
      when(() => repo.mcpServers()).thenAnswer((_) async => McpServers(servers: [server('linear', auth: McpAuthStatus.authorized)], missing: const []));
      when(() => repo.logoutMcp('linear')).thenAnswer((_) async => McpServers(servers: [server('linear', auth: McpAuthStatus.needsLogin)], missing: const []));
      await pump(tester);
      await tester.tap(find.byKey(const ValueKey('mcp-logout-linear')));
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(TextButton, 'Log out').last);
      await tester.pumpAndSettle();
      verify(() => repo.logoutMcp('linear')).called(1);
      expect(find.text('Needs login'), findsOneWidget);
    });

    testWidgets('a server without the login methods hides the login', (tester) async {
      when(() => repo.mcpServers()).thenAnswer((_) async => McpServers(servers: [server('linear', auth: McpAuthStatus.needsLogin)], missing: const []));
      when(() => repo.startMcpLogin('linear'))
          .thenThrow(const ProtocolException(ErrorCode.invalidRequest, 'unknown variant `mcp.login_start`'));
      await pump(tester);
      await tester.tap(find.byKey(const ValueKey('mcp-login-linear')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('mcp-auth-linear')), findsNothing);
      expect(find.byKey(const ValueKey('mcp-login-sheet')), findsNothing);
    });

    testWidgets('an auth change reloads the list', (tester) async {
      var auth = McpAuthStatus.needsLogin;
      when(() => repo.mcpServers()).thenAnswer((_) async => McpServers(servers: [server('linear', auth: auth)], missing: const []));
      await pump(tester);
      auth = McpAuthStatus.authorized;
      events.add(const Event(aggregateId: 'linear', atMs: 1, kind: EventKind.mcpAuthChanged, seq: 2));
      await tester.pumpAndSettle();
      expect(find.text('Connected'), findsOneWidget);
    });
  });
}
