import 'package:flutter/cupertino.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import '../data/activity.dart';
import '../features/activity/activity_chip.dart';

import '../core/theme/tokens.dart';
import '../core/widgets/ios.dart';
import '../features/connection/connection_banner.dart';
import '../features/connection/connection_bloc.dart';
import '../features/inbox/inbox_bloc.dart';
import '../features/sessions/sessions_page.dart' show openNewSession;

/// The route pattern of each tab, in the order of [shellTabs]; what the screen counter counts a tab by.
const shellTabPatterns = ['/sessions', '/inbox', '/board', '/profile'];

/// The bottom tabs, in this order (branch indices of the router's shell).
/// Sessions comes first and is where the app opens (ticket #71: the Team chat
/// is gone); the way to the chat with an employee is Profile › Employees.
/// Profile replaced More (ticket #72): account and settings on one page.
const shellTabs = [
  GlassTab(CupertinoIcons.chat_bubble_2, 'Sessions'),
  GlassTab(CupertinoIcons.tray, 'Inbox'),
  GlassTab(CupertinoIcons.square_split_2x1, 'Board'),
  GlassTab(CupertinoIcons.person_crop_circle, 'Profile'),
];

/// The inbox tab, with the badge of what waits for you (its index in [shellTabs]).
const inboxTab = 1;

/// Tabs: Sessions · Inbox · Board · Profile in a floating glass tab bar,
/// with the connection banner on top. Hardware keyboards (iPad, Android with a
/// keyboard): ⌘/Ctrl+1…4 switch tabs, ⌘/Ctrl+N opens New session.
class HomeShell extends StatefulWidget {
  const HomeShell({super.key, required this.shell});

  final StatefulNavigationShell shell;

  @override
  State<HomeShell> createState() => _HomeShellState();
}

class _HomeShellState extends State<HomeShell> {
  StatefulNavigationShell get shell => widget.shell;

  void _go(int index) => shell.goBranch(index, initialLocation: index == shell.currentIndex);

  @override
  Widget build(BuildContext context) {
    final badge = context.select<InboxBloc, int>((b) => b.state.badge);
    final bannerVisible = context.select<ConnectionBloc, bool>((b) => ConnectionBanner.visible(b.state));
    final activityOn = context.select<ConnectionBloc, bool>((b) => b.state.has(activityOverviewFeature));
    final digits = [LogicalKeyboardKey.digit1, LogicalKeyboardKey.digit2, LogicalKeyboardKey.digit3, LogicalKeyboardKey.digit4];
    final shortcuts = <ShortcutActivator, VoidCallback>{
      for (final (i, key) in digits.indexed) ...{
        SingleActivator(key, meta: true): () => _go(i),
        SingleActivator(key, control: true): () => _go(i),
      },
      const SingleActivator(LogicalKeyboardKey.keyN, meta: true): () => openNewSession(context),
      const SingleActivator(LogicalKeyboardKey.keyN, control: true): () => openNewSession(context),
    };
    return CallbackShortcuts(
      bindings: shortcuts,
      child: Focus(
        autofocus: true,
        child: TabScaffold(
          background: Tokens.bg,
          banners: [const ConnectionBanner(), if (activityOn) ActivityChip(underBanner: bannerVisible)],
          bannerVisible: bannerVisible || activityOn,
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
