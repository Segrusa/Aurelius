import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Send a refresh and wait until the bloc has reloaded (so the pull-to-refresh
/// spinner lasts as long as the reload, not a blink). Never longer than a few
/// seconds: an unchanged state may not be emitted again.
Future<void> untilReloaded<S>(StateStreamable<S> bloc, VoidCallback send, {bool Function(S state)? busy}) async {
  final done = bloc.stream.firstWhere((s) => !(busy?.call(s) ?? false)).timeout(const Duration(seconds: 6), onTimeout: () => bloc.state);
  send();
  try {
    await done;
  } catch (_) {}
}

/// Pull-to-refresh around any content, also when it is short or empty (an
/// empty list is exactly when one pulls).
class PullToRefresh extends StatelessWidget {
  const PullToRefresh({super.key, required this.onRefresh, required this.child, this.scrollable = false});

  final Future<void> Function() onRefresh;
  final Widget child;

  /// The child scrolls itself (a ListView): it is used as is.
  final bool scrollable;

  @override
  Widget build(BuildContext context) {
    return RefreshIndicator(
      onRefresh: onRefresh,
      child: scrollable
          ? child
          : LayoutBuilder(
              builder: (context, box) => ListView(
                physics: const AlwaysScrollableScrollPhysics(),
                children: [ConstrainedBox(constraints: BoxConstraints(minHeight: box.maxHeight), child: child)],
              ),
            ),
    );
  }
}
