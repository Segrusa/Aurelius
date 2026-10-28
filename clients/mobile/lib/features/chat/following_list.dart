import 'dart:math' as math;

import 'package:flutter/cupertino.dart';
import 'package:flutter/rendering.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';

/// One row of a [FollowingList]: a stable key (message id) and its widget.
class FollowItem {
  const FollowItem(this.key, this.child);

  final String key;
  final Widget child;
}

/// How close to the end (in pixels) still counts as "at the end".
const followSlack = 64.0;

/// A conversation list that follows new text only while the reader is at the
/// end. Scrolled up, nothing moves: new messages and a growing answer land
/// below the viewport, and a glass button leads back down.
///
/// Rows that were there when the list opened sit above the scroll anchor and
/// grow upward; everything newer (and the answer streaming at that moment)
/// sits below it and grows downward, so text arriving under the reader never
/// shifts what they read. Following keeps the position glued to the end
/// during layout, so it never lags a frame behind.
class FollowingList extends StatefulWidget {
  const FollowingList({super.key, required this.items, this.liveFrom, this.resetKey, this.padding = const EdgeInsets.fromLTRB(16, 12, 16, 12)});

  final List<FollowItem> items;

  /// Index of the first row that may still grow (the streaming answer);
  /// only used when the list first fills. Null: all rows are settled.
  final int? liveFrom;

  /// Another conversation: start again at the end.
  final Object? resetKey;

  final EdgeInsets padding;

  @override
  State<FollowingList> createState() => FollowingListState();
}

class FollowingListState extends State<FollowingList> {
  final _center = UniqueKey();
  final _above = GlobalKey();
  late final _controller = _FollowController(() => _following, () => (_above.currentContext?.findRenderObject() as RenderSliver?)?.geometry?.scrollExtent);
  bool _following = true;
  bool _fresh = false;
  double? _lastMax;
  bool _toEnd = false;

  /// The last row above the anchor, once decided.
  String? _edge;
  bool _settled = false;

  bool get following => _following;

  @override
  void didUpdateWidget(FollowingList oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.resetKey != widget.resetKey) {
      _edge = null;
      _settled = false;
      _following = true;
      _fresh = false;
      _lastMax = null;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  /// Back to the end and follow again (the button, or sending a message).
  void scrollToEnd() {
    setState(() {
      _following = true;
      _fresh = false;
    });
    if (!_controller.hasClients) return;
    final position = _controller.position;
    // Far up, the end is only an estimate: jump to the last screen first so
    // the animation aims at real rows.
    _toEnd = true;
    if (position.maxScrollExtent - position.pixels > position.viewportDimension * 1.5) position.jumpTo(position.maxScrollExtent - position.viewportDimension);
    position.animateTo(position.maxScrollExtent, duration: const Duration(milliseconds: 250), curve: Curves.easeOutCubic).whenComplete(() {
      _toEnd = false;
      if (!mounted || !_following || !_controller.hasClients) return;
      final end = _controller.position;
      // Not when the reader took over with a drag.
      if (!end.isScrollingNotifier.value) end.jumpTo(end.maxScrollExtent);
    });
  }

  int _split(List<FollowItem> items) {
    if (_settled) {
      if (_edge == null) return 0;
      final i = items.indexWhere((x) => x.key == _edge);
      if (i >= 0) return i + 1;
    }
    if (items.isEmpty) return 0;
    final split = (widget.liveFrom ?? items.length).clamp(0, items.length);
    _edge = split > 0 ? items[split - 1].key : null;
    _settled = true;
    return split;
  }

  void _setFollowing(bool value) {
    if (value == _following && !(value && _fresh)) return;
    setState(() {
      _following = value;
      if (value) _fresh = false;
    });
  }

  bool _onScroll(ScrollNotification n) {
    // Only real scrolling (drag, fling, animation) changes whether we follow;
    // content growing under the reader does not.
    // On the way to the end, the extent shrinks from an estimate to the real
    // one; that is no reason to stop following unless the reader drags.
    if (_toEnd && n is ScrollUpdateNotification && n.dragDetails == null) return false;
    if (n.depth == 0 && n is ScrollUpdateNotification) _setFollowing(n.metrics.maxScrollExtent - n.metrics.pixels <= followSlack);
    return false;
  }

  bool _onMetrics(ScrollMetricsNotification n) {
    if (n.depth != 0) return false;
    final max = n.metrics.maxScrollExtent;
    if (!_following && _lastMax != null && max > _lastMax! + 1 && !_fresh) setState(() => _fresh = true);
    _lastMax = max;
    return false;
  }

  @override
  Widget build(BuildContext context) {
    final items = widget.items;
    final split = _split(items);
    final above = items.sublist(0, split);
    final below = items.sublist(split);
    final p = widget.padding;
    return Stack(
      children: [
        NotificationListener<ScrollMetricsNotification>(
          onNotification: _onMetrics,
          child: NotificationListener<ScrollNotification>(
            onNotification: _onScroll,
            child: CustomScrollView(
              controller: _controller,
              center: _center,
              anchor: 1.0,
              keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
              slivers: [
                // Grows upward from the anchor: nearest row first.
                SliverPadding(
                  key: _above,
                  padding: EdgeInsets.fromLTRB(p.left, p.top, p.right, 0),
                  sliver: SliverList(
                    delegate: SliverChildBuilderDelegate(
                      (context, i) => KeyedSubtree(key: ValueKey(above[above.length - 1 - i].key), child: above[above.length - 1 - i].child),
                      childCount: above.length,
                      findChildIndexCallback: (key) {
                        final i = above.indexWhere((x) => ValueKey(x.key) == key);
                        return i < 0 ? null : above.length - 1 - i;
                      },
                    ),
                  ),
                ),
                SliverPadding(
                  key: _center,
                  padding: EdgeInsets.fromLTRB(p.left, 0, p.right, p.bottom),
                  sliver: SliverList(
                    delegate: SliverChildBuilderDelegate(
                      (context, i) => KeyedSubtree(key: ValueKey(below[i].key), child: below[i].child),
                      childCount: below.length,
                      findChildIndexCallback: (key) {
                        final i = below.indexWhere((x) => ValueKey(x.key) == key);
                        return i < 0 ? null : i;
                      },
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
        if (!_following)
          Positioned(
            right: 16,
            bottom: 12,
            child: _ToEndButton(fresh: _fresh, onTap: scrollToEnd),
          ),
      ],
    );
  }
}

/// Glass round button back to the end; a dot when new text came in.
class _ToEndButton extends StatelessWidget {
  const _ToEndButton({required this.fresh, required this.onTap});

  final bool fresh;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: fresh ? 'New text, scroll to the end' : 'Scroll to the end',
        child: GestureDetector(
          key: const Key('scroll-to-end'),
          onTap: onTap,
          child: Stack(
            clipBehavior: Clip.none,
            children: [
              Glass(
                radius: 20,
                child: SizedBox(width: 40, height: 40, child: Icon(CupertinoIcons.arrow_down, size: 18, color: fresh ? Tokens.accentText : Tokens.label)),
              ),
              if (fresh)
                Positioned(
                  key: const Key('scroll-to-end-fresh'),
                  right: 1,
                  top: 1,
                  child: Container(width: 10, height: 10, decoration: BoxDecoration(color: Tokens.accent, shape: BoxShape.circle, border: Border.all(color: Tokens.bg, width: 1.5))),
                ),
            ],
          ),
        ),
      );
}

class _FollowController extends ScrollController {
  _FollowController(this.following, this.aboveExtent);

  final bool Function() following;
  final double? Function() aboveExtent;

  @override
  ScrollPosition createScrollPosition(ScrollPhysics physics, ScrollContext context, ScrollPosition? oldPosition) =>
      _FollowPosition(physics: physics, context: context, oldPosition: oldPosition, following: following, aboveExtent: aboveExtent);
}

/// Sticks to the end during layout while following and nobody scrolls.
class _FollowPosition extends ScrollPositionWithSingleContext {
  _FollowPosition({required super.physics, required super.context, super.oldPosition, required this.following, required this.aboveExtent});

  final bool Function() following;

  /// Height of the rows above the anchor, once laid out.
  final double? Function() aboveExtent;

  @override
  bool applyContentDimensions(double minScrollExtent, double maxScrollExtent) {
    // With the anchor at the bottom the viewport lets the reader scroll back
    // to offset 0 even when the rows above it are shorter than the screen:
    // in a short conversation that showed empty space above the first
    // message, with the newest rows pushed out at the bottom. The real top is
    // where the first row reaches the top of the screen, never past the end.
    final above = aboveExtent();
    if (above != null && hasViewportDimension) minScrollExtent = math.max(minScrollExtent, math.min(maxScrollExtent, viewportDimension - above));
    final stick = following() && activity is IdleScrollActivity && hasPixels && (pixels - maxScrollExtent).abs() > 0.5;
    if (stick) correctPixels(maxScrollExtent);
    final applied = super.applyContentDimensions(minScrollExtent, maxScrollExtent);
    return applied && !stick;
  }
}
