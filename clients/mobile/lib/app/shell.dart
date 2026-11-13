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
import '../features/dashboard/give_work.dart';
import '../features/inbox/inbox_bloc.dart';
import '../features/team/channels_bloc.dart';

/// The route pattern of each tab, in the order of [shellTabs]; what the screen counter counts a tab by.
const shellTabPatterns = ['/team', '/board', '/inbox', '/more'];

/// The bottom tabs, in this order (branch indices of the router's shell).
/// Team chat comes first and is where the app opens (ticket #19); the way to the chat with an employee is More › Employees.
const shellTabs = [
  GlassTab(CupertinoIcons.chat_bubble_2, 'Team'),
  GlassTab(CupertinoIcons.square_split_2x1, 'Board'),
  GlassTab(CupertinoIcons.tray, 'Inbox'),
  GlassTab(CupertinoIcons.ellipsis_circle, 'More'),
];

/// The inbox tab, with the badge of what waits for you (its index in [shellTabs]).
const inboxTab = 2;

/// The Team tab: the mentions of a board member over the channels, a dot for anything unread (B8).
const teamTab = 0;

({int mentions, bool unread}) _teamBadge(BuildContext context) {
  try {
    return context.select<ChannelsBloc, ({int mentions, bool unread})>((b) => b.state.badge);
  } on ProviderNotFoundException {
    return (mentions: 0, unread: false);
  }
}

/// Tabs: Team · Board · Inbox · More in a floating glass tab bar,
/// with the connection banner on top. Hardware keyboards (iPad, Android with a
/// keyboard): ⌘/Ctrl+1…4 switch tabs, ⌘/Ctrl+N gives work (a new task).
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
    final team = _teamBadge(context);
    final bannerVisible = context.select<ConnectionBloc, bool>((b) => ConnectionBanner.visible(b.state));
    final activityOn = context.select<ConnectionBloc, bool>((b) => b.state.has(activityOverviewFeature));
    final digits = [LogicalKeyboardKey.digit1, LogicalKeyboardKey.digit2, LogicalKeyboardKey.digit3, LogicalKeyboardKey.digit4];
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
          banners: [const ConnectionBanner(), if (activityOn) ActivityChip(underBanner: bannerVisible)],
          bannerVisible: bannerVisible || activityOn,
          tabBar: GlassTabBar(
            current: shell.currentIndex,
            onTap: _go,
            fade: Tokens.bg,
            tabs: [
              for (final (i, t) in shellTabs.indexed)
                i == inboxTab
                    ? GlassTab(t.icon, t.label, badge: badge)
                    : i == teamTab
                        ? GlassTab(t.icon, t.label, badge: team.mentions, dot: team.unread)
                        : t,
            ],
          ),
          child: shell,
        ),
      ),
    );
  }
}
