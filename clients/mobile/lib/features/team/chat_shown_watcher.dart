import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../chat/chat_bloc.dart';

/// Tells the Team chat's [ChatBloc] whether it is on screen: the shell turns
/// the tickers of hidden tabs (and of pages under another one) off, and a
/// phone in the background shows nothing either. Only a chat on screen is read,
/// for reading a channel marks it read on the server.
class ChatShownWatcher extends StatefulWidget {
  const ChatShownWatcher({super.key, required this.child});

  final Widget child;

  @override
  State<ChatShownWatcher> createState() => _ChatShownWatcherState();
}

class _ChatShownWatcherState extends State<ChatShownWatcher> with WidgetsBindingObserver {
  bool? _shown;
  bool _ticking = true;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  bool get _foreground => switch (WidgetsBinding.instance.lifecycleState) {
        null || AppLifecycleState.resumed => true,
        _ => false,
      };

  void _tell() {
    final shown = _ticking && _foreground;
    if (shown == _shown) return;
    _shown = shown;
    context.read<ChatBloc>().add(ChatShown(shown));
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _ticking = TickerMode.valuesOf(context).enabled;
    _tell();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) => _tell();

  @override
  Widget build(BuildContext context) => widget.child;
}
