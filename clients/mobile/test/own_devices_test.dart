import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/push/push_service.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/protocol/protocol.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/account/devices.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/hosts/hosts_bloc.dart';
import 'package:aurelius_mobile/features/profile/profile_page.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'support/fake_daemon.dart' show testHost;
import 'support/tickets.dart';

// Profile → Account → Your devices: a board member's own paired devices (feature `own_devices`).

const _alex = UserView(id: 'u-1', username: 'alex', createdAtMs: 0);

RemoteClientView _device(String id, String name, {String platform = 'android', bool connected = false, bool push = false, int? lastSeenMs}) =>
    RemoteClientView(id: id, name: name, platform: platform, capabilities: const [], createdAtMs: 1, lastSeenMs: lastSeenMs, connected: connected, push: push, pairedBy: 'u-1');

void main() {
  test('a device line says the platform, whether it is connected and whether it has notifications', () {
    expect(deviceLine(_device('a', 'x', platform: 'ios', connected: true, push: true)), 'iPhone · Connected now · Notifications on');
    expect(deviceLine(_device('a', 'x')), 'Android · Never connected');
    expect(deviceLine(_device('a', 'x', platform: 'linux', lastSeenMs: DateTime(2026, 11, 2).millisecondsSinceEpoch)), 'Linux · Last seen Nov 2');
  });

  group('the page', () {
    late MockRepository repo;

    setUp(() {
      repo = MockRepository();
      when(() => repo.host).thenReturn(testHost);
      when(() => repo.devices()).thenAnswer((_) async => [_device(testHost.clientId, 'This Pixel', connected: true), _device('c-2', 'Laptop', platform: 'linux')]);
      when(() => repo.revokeDevice(any())).thenAnswer((_) async {});
    });

    Future<void> pump(WidgetTester tester) async {
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: MaterialApp(theme: aureliusTheme(), home: const DevicesPage()),
      ));
      await tester.pumpAndSettle();
    }

    testWidgets('lists the own devices, marks this phone and revokes another after a confirmation', (tester) async {
      await pump(tester);
      expect(find.text('This Pixel'), findsOneWidget);
      expect(find.text('This phone'), findsOneWidget);
      expect(find.byKey(ValueKey('revoke-device-${testHost.clientId}')), findsNothing, reason: 'this phone signs out instead');
      await tester.tap(find.byKey(const ValueKey('revoke-device-c-2')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Revoke').last);
      await tester.pumpAndSettle();
      verify(() => repo.revokeDevice('c-2')).called(1);
      expect(find.text('Laptop'), findsNothing);
    });

    testWidgets('a failed read shows what failed and Try again', (tester) async {
      when(() => repo.devices()).thenThrow(Exception('offline'));
      await pump(tester);
      expect(find.byKey(const ValueKey('devices-error')), findsOneWidget);
      expect(find.text('Try again'), findsOneWidget);
    });
  });

  group('Profile', () {
    Future<void> pump(WidgetTester tester, LinkState link) async {
      await tester.binding.setSurfaceSize(const Size(390, 2000));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final repo = MockRepository();
      when(() => repo.host).thenReturn(testHost);
      when(() => repo.linkState).thenReturn(link);
      when(() => repo.serverNameChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      final connection = MockConnectionBloc();
      whenListen(connection, const Stream<LinkState>.empty(), initialState: link);
      final company = MockCompanyBloc();
      whenListen(company, const Stream<CompanyState>.empty(), initialState: const CompanyState(loading: false));
      final hosts = MockHostsBloc();
      whenListen(hosts, const Stream<HostsState>.empty(), initialState: const HostsState(loading: false));
      final router = GoRouter(routes: [
        GoRoute(path: '/', builder: (_, _) => const ProfilePage()),
        GoRoute(path: '/account/devices', builder: (_, _) => const Scaffold(body: Text('page devices'))),
      ]);
      addTearDown(router.dispose);
      await tester.pumpWidget(MultiRepositoryProvider(
        providers: [RepositoryProvider<AureliusRepository>.value(value: repo), RepositoryProvider<PushService>.value(value: PushService())],
        child: MultiBlocProvider(
          providers: [BlocProvider<ConnectionBloc>.value(value: connection), BlocProvider<CompanyBloc>.value(value: company), BlocProvider<HostsBloc>.value(value: hosts)],
          child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router),
        ),
      ));
      await tester.pumpAndSettle();
    }

    testWidgets('shows Your devices under Account only when the server has the feature', (tester) async {
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: true, user: _alex));
      expect(find.byKey(const ValueKey('account-devices')), findsNothing);
      await pump(tester, const LinkState(status: LinkStatus.online, boardMembersExist: true, user: _alex, features: [ownDevicesFeature]));
      await tester.tap(find.byKey(const ValueKey('account-devices')));
      await tester.pumpAndSettle();
      expect(find.text('page devices'), findsOneWidget);
    });
  });
}
