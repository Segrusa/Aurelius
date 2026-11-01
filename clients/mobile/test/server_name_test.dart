import 'dart:async';

import 'package:aurelius_mobile/core/hosts/host_store.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/core/push/push_service.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/hosts/hosts_bloc.dart';
import 'package:aurelius_mobile/features/more/settings_page.dart';
import 'package:aurelius_mobile/features/power/power_bloc.dart';
import 'package:aurelius_mobile/features/power/power_widgets.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:aurelius_mobile/features/server_name/server_name_bloc.dart';
import 'package:aurelius_mobile/features/server_name/server_name_section.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/fake_daemon.dart';

class MockRepository extends Mock implements AureliusRepository {}

class MockRuntimeBloc extends MockBloc<RuntimeEvent, RuntimeState> implements RuntimeBloc {}

class MockPowerBloc extends MockBloc<PowerEvent, PowerBlocState> implements PowerBloc {}

const _owner = ['read', 'chat', 'task_control', 'approve', 'owner'];
const _me = UserView(createdAtMs: 0, id: 'u-1', username: 'owner');

ServerNameView nameOf(String name, {String? chosen, String computer = 'my-pc', int version = 0}) => ServerNameView(name: name, chosen: chosen, computerName: computer, version: version);

LinkState linkOf({List<String> features = const [serverNameFeature], List<String> capabilities = _owner, UserView? user = _me}) => LinkState(status: LinkStatus.online, features: features, capabilities: capabilities).withBoardMember(exists: true, user: user);

void main() {
  group('Settings → Server name', () {
    late MockRepository repo;
    late StreamController<ServerNameView> changes;
    late ServerNameView current;

    setUpAll(() => registerFallbackValue(''));

    setUp(() {
      repo = MockRepository();
      changes = StreamController<ServerNameView>.broadcast();
      current = nameOf('Office', chosen: 'Office', version: 1);
      when(() => repo.host).thenReturn(testHost);
      when(() => repo.serverNameChanges).thenAnswer((_) => changes.stream);
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.serverName()).thenAnswer((_) async => current);
    });

    tearDown(() => changes.close());

    Future<void> pump(WidgetTester tester, LinkState link) async {
      when(() => repo.linkState).thenReturn(link);
      await tester.pumpWidget(
        RepositoryProvider<AureliusRepository>.value(
          value: repo,
          child: MaterialApp(
            theme: aureliusTheme(),
            home: Scaffold(
              body: BlocProvider(
                create: (_) => ServerNameBloc(repo)..add(const ServerNameRefreshed()),
                child: ListView(children: [ServerNameSection(link: link)]),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
    }

    Finder edit() => find.byKey(const ValueKey('server-name-edit'));

    testWidgets('shows the chosen name with the computer as the default (AC1)', (tester) async {
      await pump(tester, linkOf());
      expect(find.text('Server name'), findsOneWidget);
      expect(find.text('Office'), findsOneWidget);
      expect(find.text('Default: my-pc'), findsOneWidget);
      expect(edit(), findsOneWidget);
    });

    testWidgets('without a chosen name: the computer name, no default line', (tester) async {
      current = nameOf('my-pc');
      await pump(tester, linkOf());
      expect(find.text('my-pc'), findsOneWidget);
      expect(find.textContaining('Default:'), findsNothing);
    });

    testWidgets('edit and save sends the new name and closes the dialog', (tester) async {
      when(() => repo.setServerName(any())).thenAnswer((_) async => const CommandResultServerNameSet(name: 'Thuis-PC', version: 2));
      await pump(tester, linkOf());
      await tester.tap(edit());
      await tester.pumpAndSettle();
      expect(find.text("Leave empty to use the computer's name (my-pc)."), findsOneWidget);
      expect(tester.widget<TextField>(find.byKey(const ValueKey('server-name-field'))).controller!.text, 'Office');
      await tester.enterText(find.byKey(const ValueKey('server-name-field')), 'Thuis-PC');
      await tester.tap(find.byKey(const ValueKey('server-name-save')));
      await tester.pumpAndSettle();
      verify(() => repo.setServerName('Thuis-PC')).called(1);
      expect(find.byKey(const ValueKey('server-name-field')), findsNothing);
      // The connection's read after server.renamed reaches the row (SN-02 AC1).
      changes.add(nameOf('Thuis-PC', chosen: 'Thuis-PC', version: 2));
      await tester.pumpAndSettle();
      expect(find.text('Thuis-PC'), findsOneWidget);
      expect(find.text('Default: my-pc'), findsOneWidget);
    });

    testWidgets('saving it empty brings the default back (AC3)', (tester) async {
      when(() => repo.setServerName(any())).thenAnswer((_) async => const CommandResultServerNameSet(name: 'my-pc', version: 2));
      await pump(tester, linkOf());
      await tester.tap(edit());
      await tester.pumpAndSettle();
      await tester.enterText(find.byKey(const ValueKey('server-name-field')), '');
      await tester.tap(find.byKey(const ValueKey('server-name-save')));
      await tester.pumpAndSettle();
      verify(() => repo.setServerName('')).called(1);
      changes.add(nameOf('my-pc', version: 2));
      await tester.pumpAndSettle();
      expect(find.text('my-pc'), findsOneWidget);
      expect(find.textContaining('Default:'), findsNothing);
    });

    testWidgets("the server's refusal stays in the dialog with its text (AC4)", (tester) async {
      when(() => repo.setServerName(any())).thenThrow(const ProtocolException(ErrorCode.validation, 'The server name cannot hold line breaks or control characters.'));
      await pump(tester, linkOf());
      await tester.tap(edit());
      await tester.pumpAndSettle();
      await tester.enterText(find.byKey(const ValueKey('server-name-field')), 'Two\nlines');
      await tester.tap(find.byKey(const ValueKey('server-name-save')));
      await tester.pumpAndSettle();
      expect(find.text('The server name cannot hold line breaks or control characters.'), findsOneWidget);
      expect(find.byKey(const ValueKey('server-name-field')), findsOneWidget, reason: 'the dialog stays open');
      await tester.tap(find.byKey(const ValueKey('server-name-cancel')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('server-name-field')), findsNothing);
      expect(find.text('Office'), findsOneWidget, reason: 'the old name stays');
    });

    testWidgets('forbidden shows in the dialog too', (tester) async {
      when(() => repo.setServerName(any())).thenThrow(const ProtocolException(ErrorCode.forbidden, 'This needs the Owner right.'));
      await pump(tester, linkOf());
      await tester.tap(edit());
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('server-name-save')));
      await tester.pumpAndSettle();
      expect(find.text('This needs the Owner right.'), findsOneWidget);
    });

    testWidgets('no Edit without a board member', (tester) async {
      await pump(tester, linkOf(user: null));
      expect(find.text('Office'), findsOneWidget);
      expect(edit(), findsNothing);
    });

    testWidgets('no Edit without the Owner right (a phone, A-04)', (tester) async {
      await pump(tester, linkOf(capabilities: const ['read', 'chat', 'task_control', 'approve']));
      expect(find.text('Office'), findsOneWidget);
      expect(edit(), findsNothing);
    });

    testWidgets('an older server: no section, no Edit, and server.get_name is never asked (SN-02 AC4)', (tester) async {
      await pump(tester, linkOf(features: const []));
      expect(find.text('Server name'), findsNothing);
      expect(edit(), findsNothing);
      verifyNever(() => repo.serverName());
    });

    testWidgets('a failed read says so, with Try again', (tester) async {
      when(() => repo.serverName()).thenThrow(const DisconnectedException());
      await pump(tester, linkOf());
      expect(find.textContaining("Could not read the server's name."), findsOneWidget);
      when(() => repo.serverName()).thenAnswer((_) async => current);
      await tester.tap(find.byKey(const ValueKey('server-name-retry')));
      await tester.pumpAndSettle();
      expect(find.text('Default: my-pc'), findsOneWidget);
    });
  });

  group('Settings follows server.renamed live (SN-02 AC1)', () {
    testWidgets('the title, the server name row and the Power section show the new name', (tester) async {
      tester.view.physicalSize = const Size(1170, 2532);
      tester.view.devicePixelRatio = 3;
      addTearDown(tester.view.reset);
      final daemon = FakeDaemon()
        ..features = const [serverNameFeature]
        ..capabilities = _owner;
      late MemoryHostStore store;
      late AureliusConnection connection;
      await tester.runAsync(() async {
        store = await storeWithSecret();
        daemon.addUser('u-1', 'owner', 'a long password');
        await store.setSessionToken(testHost.hostId, daemon.sessionFor('u-1'));
        connection = AureliusConnection(host: testHost, store: store, api: FakeApi(), connect: daemon.connect)..start();
        await until(() => connection.state.isOnline);
      });
      final repository = AureliusRepository(connection);
      final runtime = MockRuntimeBloc();
      when(() => runtime.state).thenReturn(const RuntimeState());
      final power = MockPowerBloc();
      when(() => power.state).thenReturn(const PowerBlocState());
      await tester.pumpWidget(
        MultiRepositoryProvider(
          providers: [
            RepositoryProvider<AureliusRepository>.value(value: repository),
            RepositoryProvider<PushService>.value(value: PushService()),
          ],
          child: MultiBlocProvider(
            providers: [
              BlocProvider(create: (_) => ConnectionBloc(repository)),
              BlocProvider(create: (_) => HostsBloc(store)..add(const HostsStarted())),
              BlocProvider<RuntimeBloc>.value(value: runtime),
              BlocProvider<PowerBloc>.value(value: power),
            ],
            child: MaterialApp(theme: aureliusTheme(), home: const SettingsPage()),
          ),
        ),
      );
      await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 50)));
      await tester.pump();
      expect(find.text('my-pc'), findsWidgets);

      await tester.runAsync(() async {
        daemon
          ..chosenName = 'Thuis-PC'
          ..nameVersion = 1;
        daemon.event(daemon.current!, 'server.renamed', seq: 31);
        await until(() => connection.state.hostName == 'Thuis-PC');
        await Future<void>.delayed(const Duration(milliseconds: 20));
      });
      await tester.pump();
      await tester.pump();
      expect(find.text('Thuis-PC'), findsNWidgets(2), reason: 'the title and the Server name row');
      expect(find.text('Default: my-pc'), findsOneWidget);
      expect(find.byKey(const ValueKey('server-name-edit')), findsOneWidget);
      expect((await tester.runAsync(store.hosts))!.single.name, 'Thuis-PC');

      // The Power section's confirms name the server it gets (power_test checks the text).
      expect(tester.widget<PowerSection>(find.byType(PowerSection)).serverName, 'Thuis-PC');

      await tester.runAsync(connection.dispose);
    });
  });
}
