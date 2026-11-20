import 'package:flutter/cupertino.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import '../core/theme/tokens.dart';
import '../core/widgets/ios.dart';
import '../features/connection/connection_banner.dart';
import '../features/connection/connection_bloc.dart';
import '../features/inbox/inbox_bloc.dart';

/// The route pattern of each tab, in the order of [shellTabs]; what the screen counter counts a tab by.
const shellTabPatterns = ['/chat', '/sessions', '/inbox', '/profile'];

/// The bottom tabs, in this order (branch indices of the router's shell).
/// Chat comes first and is where the app opens: what to start next, as the
/// desktop's New session. Sessions lists your sessions and open tickets, Inbox
/// what needs you, Profile your account and settings. The Board is not on
/// the phone; a project still shows its own.
const shellTabs = [
  GlassTab(CupertinoIcons.chat_bubble_text, 'Chat', symbol: 'bubble.left.and.text.bubble.right'),
  GlassTab(CupertinoIcons.list_bullet, 'Sessions', symbol: 'list.bullet.rectangle'),
  GlassTab(CupertinoIcons.tray, 'Inbox', symbol: 'tray'),
  GlassTab(CupertinoIcons.person_crop_circle, 'Profile', symbol: 'person.crop.circle'),
];

/// The Chat tab: where ⌘/Ctrl+N goes.
const chatTab = 0;

/// The inbox tab, with the badge of what waits for you (its index in [shellTabs]).
const inboxTab = 2;

/// Tabs: Chat · Sessions · Inbox · Profile in the system tab bar on iOS (a
/// floating glass bar elsewhere), with the connection banner on top. Hardware
/// keyboards (iPad, Android with a keyboard): ⌘/Ctrl+1…4 switch tabs,
/// ⌘/Ctrl+N opens Chat.
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
    final digits = [LogicalKeyboardKey.digit1, LogicalKeyboardKey.digit2, LogicalKeyboardKey.digit3, LogicalKeyboardKey.digit4];
    final shortcuts = <ShortcutActivator, VoidCallback>{
      for (final (i, key) in digits.indexed) ...{
        SingleActivator(key, meta: true): () => _go(i),
        SingleActivator(key, control: true): () => _go(i),
      },
      const SingleActivator(LogicalKeyboardKey.keyN, meta: true): () => _go(chatTab),
      const SingleActivator(LogicalKeyboardKey.keyN, control: true): () => _go(chatTab),
    };
    return CallbackShortcuts(
      bindings: shortcuts,
      child: Focus(
        autofocus: true,
        child: TabScaffold(
          background: Tokens.bg,
          banners: const [ConnectionBanner()],
          bannerVisible: bannerVisible,
          tabBar: AppTabBar(
            current: shell.currentIndex,
            onTap: _go,
            fade: Tokens.bg,
            tabs: [
              for (final (i, t) in shellTabs.indexed) i == inboxTab ? GlassTab(t.icon, t.label, badge: badge, symbol: t.symbol) : t,
            ],
          ),
          child: shell,
        ),
      ),
    );
  }
}
