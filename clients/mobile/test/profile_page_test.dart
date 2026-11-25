import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/push/push_service.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
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

import 'support/builders.dart';
import 'support/fake_daemon.dart' show testHost;
import 'support/tickets.dart';

// Ticket #72: the Profile tab replaces More, with Account and Settings on one page.

const _alex = UserView(id: 'u-1', username: 'alex', createdAtMs: 0);
const _signedIn = LinkState(status: LinkStatus.online, boardMembersExist: true, user: _alex);

void main() {
  late MockRepository repo;

  setUp(() {
    repo = MockRepository();
    when(() => repo.host).thenReturn(testHost);
    when(() => repo.linkState).thenReturn(_signedIn);
    when(() => repo.serverNameChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
  });

  Future<GoRouter> pump(WidgetTester tester, {LinkState link = _signedIn}) async {
    await tester.binding.setSurfaceSize(const Size(390, 2000));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final connection = MockConnectionBloc();
    whenListen(connection, const Stream<LinkState>.empty(), initialState: link);
    final company = MockCompanyBloc();
    whenListen(company, const Stream<CompanyState>.empty(), initialState: CompanyState(loading: false, employees: [employeeOf(id: 'dev', name: 'Cy')]));
    final hosts = MockHostsBloc();
    whenListen(hosts, const Stream<HostsState>.empty(), initialState: const HostsState(loading: false));
    final router = GoRouter(routes: [
      GoRoute(path: '/', builder: (_, _) => const ProfilePage()),
      for (final path in ['/company', '/projects', '/account/sessions', '/account/claude', '/account/password', '/settings/board', '/settings/board-members', '/hosts'])
        GoRoute(path: path, builder: (_, _) => Scaffold(body: Text('page $path'))),
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
    return router;
  }

  testWidgets('Profile shows the board member, their account and the settings on one page', (tester) async {
    await pump(tester);
    expect(find.text('Profile'), findsWidgets);
    expect(find.byKey(const ValueKey('profile-name')), findsOneWidget);
    expect(find.text('alex'), findsWidgets);
    for (final key in ['profile-pc', 'account-sessions', 'account-claude', 'account-password', 'profile-employees', 'profile-projects', 'settings-board', 'settings-board-members', 'account-sign-out']) {
      expect(find.byKey(ValueKey(key)), findsOneWidget, reason: key);
    }
    expect(find.text('Notifications'), findsOneWidget);
    expect(find.text('Paired PCs'), findsOneWidget);
    expect(find.text('About Aurelius'), findsOneWidget);
    expect(find.text('More'), findsNothing);
    expect(find.text('Signed in on'), findsOneWidget, reason: "the desktop's words for where you are signed in");
  });

  testWidgets('the parts stand in order: you and the PC, Account, Workspace, App, then Sign out', (tester) async {
    await pump(tester);
    double y(Finder f) => tester.getTopLeft(f).dy;
    final ys = [
      y(find.byKey(const ValueKey('profile-name'))),
      y(find.byKey(const ValueKey('profile-pc'))),
      y(find.text('ACCOUNT')),
      y(find.text('WORKSPACE')),
      y(find.text('APP')),
      y(find.byKey(const ValueKey('account-sign-out'))),
    ];
    expect(ys, [...ys]..sort());
  });

  for (final (key, page) in [
    ('profile-employees', '/company'),
    ('profile-projects', '/projects'),
    ('profile-pc', '/hosts'),
    ('account-sessions', '/account/sessions'),
    ('account-claude', '/account/claude'),
    ('account-password', '/account/password'),
    ('settings-board', '/settings/board'),
    ('settings-board-members', '/settings/board-members'),
  ]) {
    testWidgets('$key opens $page', (tester) async {
      await pump(tester);
      await tester.tap(find.byKey(ValueKey(key)));
      await tester.pumpAndSettle();
      expect(find.text('page $page'), findsOneWidget);
    });
  }

  testWidgets('a server with My defaults has only My defaults; Model & effort stays for an old server (OPR-5 point 5)', (tester) async {
    const mine = LinkState(status: LinkStatus.online, boardMembersExist: true, user: _alex, features: ['user_run_defaults', 'employee_run_overview']);
    await pump(tester, link: mine);
    expect(find.byKey(const ValueKey('settings-my-defaults')), findsOneWidget);
    expect(find.byKey(const ValueKey('settings-employee-runs')), findsNothing);
    expect(find.text('Per employee'), findsNothing);

    const old = LinkState(status: LinkStatus.online, boardMembersExist: true, user: _alex, features: ['employee_run_overview']);
    await pump(tester, link: old);
    expect(find.byKey(const ValueKey('settings-my-defaults')), findsNothing);
    expect(find.byKey(const ValueKey('settings-employee-runs')), findsOneWidget);
  });

  testWidgets('without a signed-in board member there is no account part and no Sign out', (tester) async {
    await pump(tester, link: const LinkState(status: LinkStatus.online));
    expect(find.byKey(const ValueKey('account-sessions')), findsNothing);
    expect(find.byKey(const ValueKey('account-sign-out')), findsNothing);
    expect(find.byKey(const ValueKey('settings-board')), findsOneWidget, reason: 'the settings stay');
  });
}
