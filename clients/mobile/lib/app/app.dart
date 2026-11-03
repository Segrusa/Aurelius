import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../core/hosts/host.dart';
import '../core/hosts/host_store.dart';
import '../core/net/connection.dart';
import '../core/net/gateway_api.dart';
import '../core/push/push_service.dart';
import '../core/theme/theme.dart';
import '../core/theme/tokens.dart';
import '../data/aurelius_repository.dart';
import '../features/company/company_bloc.dart';
import '../features/company/project_order.dart';
import '../features/connection/connection_bloc.dart';
import '../features/hosts/hosts_bloc.dart';
import '../features/inbox/inbox_bloc.dart';
import '../features/power/power_bloc.dart';
import '../features/pairing/pair_page.dart';
import '../features/pairing/pairing_bloc.dart';
import '../features/runtime/runtime_bloc.dart';
import '../features/links/link_actions.dart' show aureliusLinkReady, pendingAureliusLink, takePendingAureliusLink;
import '../features/team/channels_bloc.dart';
import 'mobile_core.dart';
import 'router.dart';

/// Root: paired hosts decide between onboarding and the app for the active
/// host. Each host gets its own connection, read models and router; a host
/// switch throws all of it away (spec R08).
class AureliusApp extends StatelessWidget {
  const AureliusApp({super.key, required this.store, required this.api, required this.push});

  final HostStore store;
  final GatewayApi api;
  final PushService push;

  @override
  Widget build(BuildContext context) {
    return MultiRepositoryProvider(
      providers: [
        RepositoryProvider.value(value: store),
        RepositoryProvider.value(value: api),
        RepositoryProvider.value(value: push),
      ],
      child: BlocProvider(
        create: (_) => HostsBloc(store)..add(const HostsStarted()),
        child: BlocBuilder<HostsBloc, HostsState>(
          buildWhen: (a, b) => a.loading != b.loading || a.active != b.active,
          builder: (context, hosts) {
            if (hosts.loading) return const _Splash();
            final host = hosts.active;
            if (host == null) return const _Onboarding();
            return HostApp(key: ValueKey(host.hostId), host: host);
          },
        ),
      ),
    );
  }
}

class _Splash extends StatelessWidget {
  const _Splash();

  @override
  Widget build(BuildContext context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: aureliusTheme(),
        home: const Scaffold(body: Center(child: CircularProgressIndicator())),
      );
}

class _Onboarding extends StatelessWidget {
  const _Onboarding();

  @override
  Widget build(BuildContext context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        title: 'Aurelius',
        theme: aureliusTheme(),
        home: BlocProvider(
          create: (context) => PairingBloc(context.read<GatewayApi>(), platform: Platform.isIOS ? 'ios' : 'android'),
          child: const PairPage(firstRun: true),
        ),
      );
}

/// Everything for one host.
class HostApp extends StatefulWidget {
  const HostApp({super.key, required this.host});

  final Host host;

  @override
  State<HostApp> createState() => _HostAppState();
}

class _HostAppState extends State<HostApp> with WidgetsBindingObserver {
  late final AureliusConnection _connection;
  late final AureliusRepository _repository;
  late final AppRouter _router;
  late final ValueNotifier<BoardMemberStep> _boardMember;
  late final StreamSubscription<LinkState> _link;
  // Kept for dispose, where the context may no longer be read.
  late final PushService _push;

  @override
  void initState() {
    super.initState();
    _connection = AureliusConnection(host: widget.host, store: context.read<HostStore>(), api: context.read<GatewayApi>())..start();
    _repository = AureliusRepository(_connection);
    _boardMember = ValueNotifier(_connection.state.boardMemberStep);
    final hosts = context.read<HostsBloc>();
    _link = _connection.states.listen((s) {
      _boardMember.value = s.boardMemberStep;
      _openPendingLink();
      // The connection stored the server's new name: the paired PCs show it too.
      if (s.hostName != null && s.hostName != hosts.state.active?.name) hosts.add(const HostsChanged());
    });
    _router = AppRouter(widget.host, boardMember: _boardMember);
    takePendingAureliusLink = _openPendingLink;
    WidgetsBinding.instance.addPostFrameCallback((_) => _openPendingLink());
    _push = context.read<PushService>()..attach(_repository, _router);
    WidgetsBinding.instance.addObserver(this);
  }

  /// A link that asked to switch to this server (§6.3) or came from the operating
  /// system (A5b) opens once the server is connected and signed in here; one that
  /// is not a valid link says so right away. Nothing is navigated before that.
  void _openPendingLink() {
    final raw = pendingAureliusLink;
    if (raw == null) return;
    final s = _connection.state;
    if (!aureliusLinkReady(raw, online: s.isOnline, signedIn: s.user != null)) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (pendingAureliusLink == raw && _router.openLink(raw)) pendingAureliusLink = null;
    });
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    // iOS suspends sockets in the background; reconnect right away on return.
    if (state == AppLifecycleState.resumed) _connection.wake();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    // A newer host's app may already have taken over (the old one is disposed last).
    if (takePendingAureliusLink == _openPendingLink) takePendingAureliusLink = null;
    _push.detach(_repository);
    _link.cancel();
    _router.dispose();
    _boardMember.dispose();
    _connection.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return RepositoryProvider.value(
      value: _repository,
      child: MultiBlocProvider(
        providers: [
          BlocProvider(create: (_) => ConnectionBloc(_repository)),
          BlocProvider(create: (_) => InboxBloc(_repository)),
          // Only the Power banner and Settings section read it: no refresh at start while they are hidden.
          if (shownInMobileCore('power')) BlocProvider(create: (_) => PowerBloc(_repository)),
          BlocProvider(create: (_) => CompanyBloc(_repository, order: ProjectOrderStore(widget.host.hostId))),
          BlocProvider(create: (_) => RuntimeBloc(_repository)),
          BlocProvider(create: (_) => ChannelsBloc(_repository)),
        ],
        child: MaterialApp.router(
          debugShowCheckedModeBanner: false,
          title: 'Aurelius',
          theme: aureliusTheme(),
          routerConfig: _router.config,
          builder: (context, child) => widget.host.isDev
              ? Banner(message: 'DEV', location: BannerLocation.topEnd, color: Tokens.devBadge, child: child!)
              : child!,
        ),
      ),
    );
  }
}
