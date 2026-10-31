import 'package:flutter/cupertino.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../core/theme/tokens.dart';
import '../core/widgets/ios.dart';
import '../features/connection/connection_banner.dart';
import '../features/connection/connection_bloc.dart';
import '../features/dashboard/give_work.dart';
import '../features/inbox/inbox_bloc.dart';
import '../features/power/power_widgets.dart';

/// The bottom tabs, in this order (branch indices of the router's shell).
/// The Board took the place of Tasks (D-03); Tasks is under More.
const shellTabs = [
  GlassTab(CupertinoIcons.house, 'Home'),
  GlassTab(CupertinoIcons.square_split_2x1, 'Board'),
  GlassTab(CupertinoIcons.chat_bubble_2, 'Team'),
  GlassTab(CupertinoIcons.tray, 'Inbox'),
  GlassTab(CupertinoIcons.ellipsis_circle, 'More'),
];

/// The inbox tab, with the badge of what waits for you (its index in [shellTabs]).
const inboxTab = 3;

/// Tabs: Home · Board · Team · Inbox · More in a floating glass tab bar,
/// with the connection banner on top. Hardware keyboards (iPad, Android with a
/// keyboard): ⌘/Ctrl+1…5 switch tabs, ⌘/Ctrl+N gives work (a new task).
class HomeShell extends StatelessWidget {
  const HomeShell({super.key, required this.shell});

  final StatefulNavigationShell shell;

  void _go(int index) => shell.goBranch(index, initialLocation: index == shell.currentIndex);

  @override
  Widget build(BuildContext context) {
    final badge = context.select<InboxBloc, int>((b) => b.state.badge);
    final bannerVisible = context.select<ConnectionBloc, bool>((b) => ConnectionBanner.visible(b.state));
    final digits = [LogicalKeyboardKey.digit1, LogicalKeyboardKey.digit2, LogicalKeyboardKey.digit3, LogicalKeyboardKey.digit4, LogicalKeyboardKey.digit5];
    final shortcuts = <ShortcutActivator, VoidCallback>{
      for (final (i, key) in digits.indexed) ...{
        SingleActivator(key, meta: true): () => _go(i),
        SingleActivator(key, control: true): () => _go(i),
      },
      const SingleActivator(LogicalKeyboardKey.keyN, meta: true): () => giveWork(context),
      const SingleActivator(LogicalKeyboardKey.keyN, control: true): () => giveWork(context),
    };
    return CallbackShortcuts(
      bindings: shortcuts,
      child: Focus(
        autofocus: true,
        child: TabScaffold(
          background: Tokens.bg,
          banners: const [ConnectionBanner(), PowerBanner()],
          bannerVisible: bannerVisible,
          tabBar: GlassTabBar(
            current: shell.currentIndex,
            onTap: _go,
            fade: Tokens.bg,
            tabs: [
              for (final (i, t) in shellTabs.indexed) i == inboxTab ? GlassTab(t.icon, t.label, badge: badge) : t,
            ],
          ),
          child: shell,
        ),
      ),
    );
  }
}
