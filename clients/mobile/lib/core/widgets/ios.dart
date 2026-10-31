import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme/tokens.dart';
import 'widgets.dart' show InfoIcon;

/// Follows the phone's light/dark setting: sets [Tokens.dark] and rebuilds
/// everything below when it flips (the tokens are plain getters, so a theme
/// change alone would not reach widgets that read them directly).
class AppearanceScope extends StatefulWidget {
  const AppearanceScope({super.key, required this.child});

  final Widget child;

  @override
  State<AppearanceScope> createState() => _AppearanceScopeState();
}

class _AppearanceScopeState extends State<AppearanceScope> {
  @override
  Widget build(BuildContext context) {
    final dark = MediaQuery.platformBrightnessOf(context) == Brightness.dark;
    if (dark != Tokens.dark) {
      Tokens.dark = dark;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        void visit(Element e) {
          e.markNeedsBuild();
          e.visitChildren(visit);
        }

        (context as Element).visitChildren(visit);
      });
    }
    return widget.child;
  }
}

/// Liquid Glass: a blurred, translucent surface with a fine light edge.
///
/// [flat] drops the outer shadow for glass that sits in a bar (toolbar
/// buttons) instead of floating over content: the 28 pt shadow spilled out
/// of the app bar as a grey halo and band. In light mode a flat surface gets
/// a hairline edge instead, so it stays visible on a white page.
class Glass extends StatelessWidget {
  const Glass({super.key, required this.child, this.radius = 22, this.padding, this.flat = false, this.tint});

  final Widget child;
  final double radius;
  final EdgeInsets? padding;
  final bool flat;

  /// Replaces [Tokens.glass], e.g. a denser glass under text.
  final Color? tint;

  @override
  Widget build(BuildContext context) {
    final shape = BorderRadius.circular(radius);
    final edge = flat && !Tokens.dark ? Tokens.sep : Tokens.glassEdge;
    final surface = ClipRRect(
      borderRadius: shape,
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 24, sigmaY: 24),
        child: Container(
          padding: padding,
          decoration: BoxDecoration(color: tint ?? Tokens.glass, borderRadius: shape, border: Border.all(color: edge, width: 0.5)),
          child: child,
        ),
      ),
    );
    if (flat) return surface;
    return CustomPaint(painter: _OuterShadow(radius: radius, shadows: Tokens.glassShadow), child: surface);
  }
}

/// Box shadows clipped to the outside of a rounded rectangle.
class _OuterShadow extends CustomPainter {
  _OuterShadow({required this.radius, required this.shadows});

  final double radius;
  final List<BoxShadow> shadows;

  @override
  void paint(Canvas canvas, Size size) {
    final shape = RRect.fromRectAndRadius(Offset.zero & size, Radius.circular(radius));
    canvas.save();
    canvas.clipPath(Path.combine(PathOperation.difference, Path()..addRect((Offset.zero & size).inflate(80)), Path()..addRRect(shape)));
    for (final s in shadows) {
      canvas.drawRRect(shape.shift(s.offset).inflate(s.spreadRadius), s.toPaint());
    }
    canvas.restore();
  }

  @override
  bool shouldRepaint(_OuterShadow old) => old.radius != radius || old.shadows != shadows;
}

/// Toolbar button: an icon in a 44 pt glass circle.
class GlassButton extends StatelessWidget {
  const GlassButton({super.key, required this.icon, required this.onPressed, this.tooltip, this.color});

  final IconData icon;
  final VoidCallback? onPressed;
  final String? tooltip;

  /// The glyph's color; the accent for an add action.
  final Color? color;

  @override
  Widget build(BuildContext context) => Glass(
        flat: true,
        child: SizedBox.square(dimension: 44, child: IconButton(tooltip: tooltip, onPressed: onPressed, icon: Icon(icon, size: 21, color: color), padding: EdgeInsets.zero)),
      );
}

/// One choice of a [GlassMenuButton].
class GlassMenuItem<T> {
  const GlassMenuItem(this.value, this.label, {this.danger = false});

  final T value;
  final String label;

  /// Shown in red (archive, delete).
  final bool danger;
}

/// An icon button (by default the ellipsis) that opens a [showGlassMenu]
/// under itself, in place of Material's popup menu.
class GlassMenuButton<T> extends StatelessWidget {
  const GlassMenuButton({super.key, required this.items, required this.onSelected, this.icon = CupertinoIcons.ellipsis, this.tooltip});

  /// Built when the menu opens, so it follows the latest state.
  final List<GlassMenuItem<T>> Function() items;
  final ValueChanged<T> onSelected;
  final IconData icon;
  final String? tooltip;

  @override
  Widget build(BuildContext context) => IconButton(
        tooltip: tooltip,
        icon: Icon(icon),
        onPressed: () async {
          final value = await showGlassMenu<T>(context, items());
          if (value != null) onSelected(value);
        },
      );
}

/// A Liquid Glass menu beside the widget of [context]: a rounded, blurred,
/// translucent panel that grows from the button. Below it when there is
/// room, else above. Resolves to the chosen value, or null when dismissed.
Future<T?> showGlassMenu<T>(BuildContext context, List<GlassMenuItem<T>> items) {
  final box = context.findRenderObject() as RenderBox;
  final anchor = box.localToGlobal(Offset.zero) & box.size;
  return Navigator.of(context).push(_GlassMenuRoute<T>(anchor: anchor, items: items, barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel));
}

class _GlassMenuRoute<T> extends PopupRoute<T> {
  _GlassMenuRoute({required this.anchor, required this.items, required this.barrierLabel});

  final Rect anchor;
  final List<GlassMenuItem<T>> items;

  @override
  final String? barrierLabel;

  @override
  Color? get barrierColor => null;

  @override
  bool get barrierDismissible => true;

  @override
  Duration get transitionDuration => const Duration(milliseconds: 200);

  @override
  Duration get reverseTransitionDuration => const Duration(milliseconds: 140);

  static const _rowHeight = 46.0;

  @override
  Widget buildPage(BuildContext context, Animation<double> animation, Animation<double> secondaryAnimation) {
    final size = MediaQuery.sizeOf(context);
    final pad = MediaQuery.paddingOf(context);
    final right = anchor.center.dx > size.width / 2;
    final height = items.length * _rowHeight + 12;
    final below = anchor.bottom + 8 + height <= size.height - pad.bottom - 12 || anchor.top - 8 - height < pad.top;
    final alignment = below ? (right ? Alignment.topRight : Alignment.topLeft) : (right ? Alignment.bottomRight : Alignment.bottomLeft);
    final curve = CurvedAnimation(parent: animation, curve: Curves.easeOutCubic, reverseCurve: Curves.easeIn);
    return Stack(
      children: [
        Positioned(
          top: below ? anchor.bottom + 8 : null,
          bottom: below ? null : size.height - anchor.top + 8,
          right: right ? math.max(12, size.width - anchor.right) : null,
          left: right ? null : math.max(12, anchor.left),
          child: FadeTransition(
            opacity: curve,
            child: ScaleTransition(
              alignment: alignment,
              scale: Tween(begin: 0.85, end: 1.0).animate(curve),
              child: ConstrainedBox(
                constraints: const BoxConstraints(minWidth: 200, maxWidth: 280),
                child: Glass(
                  radius: 18,
                  tint: Tokens.glass.withValues(alpha: Tokens.dark ? 0.72 : 0.78),
                  padding: const EdgeInsets.symmetric(vertical: 6),
                  child: Material(
                    type: MaterialType.transparency,
                    child: IntrinsicWidth(
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          for (final item in items)
                            InkWell(
                              onTap: () => Navigator.pop(context, item.value),
                              child: Container(
                                height: _rowHeight,
                                alignment: Alignment.centerLeft,
                                padding: const EdgeInsets.symmetric(horizontal: 18),
                                child: Text(item.label, maxLines: 1, style: TextStyle(fontSize: 17, color: item.danger ? Tokens.negative : Tokens.label)),
                              ),
                            ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

/// Icon buttons in one glass capsule, as iOS 26 groups toolbar items.
class GlassCapsule extends StatelessWidget {
  const GlassCapsule({super.key, required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) => Glass(
        flat: true,
        child: IconButtonTheme(
          data: IconButtonThemeData(
            style: IconButton.styleFrom(foregroundColor: Tokens.label, fixedSize: const Size.square(44), minimumSize: const Size.square(44), padding: EdgeInsets.zero, iconSize: 21),
          ),
          child: SizedBox(height: 44, child: Row(mainAxisSize: MainAxisSize.min, children: children)),
        ),
      );
}

/// The app bar of pushed screens: a glass back button, a centered title and
/// the icon actions grouped in a glass capsule.
class GlassAppBar extends StatelessWidget implements PreferredSizeWidget {
  const GlassAppBar({super.key, this.title, this.actions = const [], this.leading, this.bottom, this.titleSpacing, this.backgroundColor});

  final Widget? title;
  final List<Widget> actions;
  final Widget? leading;
  final PreferredSizeWidget? bottom;
  final double? titleSpacing;
  final Color? backgroundColor;

  @override
  Size get preferredSize => Size.fromHeight(56 + (bottom?.preferredSize.height ?? 0));

  static bool _isIcon(Widget w) => w is IconButton || w is PopupMenuButton || w is GlassMenuButton;

  @override
  Widget build(BuildContext context) {
    final canPop = ModalRoute.of(context)?.impliesAppBarDismissal ?? false;
    final icons = actions.where(_isIcon).toList();
    final others = actions.where((w) => !_isIcon(w)).toList();
    final lead = leading ?? (canPop ? IconButton(tooltip: 'Back', icon: const Icon(CupertinoIcons.back), onPressed: () => Navigator.maybePop(context)) : null);
    return AppBar(
      toolbarHeight: 56,
      backgroundColor: backgroundColor,
      automaticallyImplyLeading: false,
      leadingWidth: lead == null ? null : 68,
      leading: lead == null ? null : Center(child: GlassCapsule(children: [lead])),
      title: title,
      titleSpacing: titleSpacing,
      bottom: bottom,
      actions: [
        ...others,
        if (icons.isNotEmpty) GlassCapsule(children: icons),
        const SizedBox(width: 16),
      ],
    );
  }
}

/// A tab page with the iOS large title: glass buttons top right, the big
/// title (and an optional subtitle) that collapses into a small centered
/// title on a glass bar as the content scrolls under it.
class LargeTitleScaffold extends StatefulWidget {
  const LargeTitleScaffold({
    super.key,
    required this.title,
    this.subtitle,
    this.actions = const [],
    this.children,
    this.slivers,
    this.fill,
    this.onRefresh,
    this.plain = false,
  });

  final String title;
  final String? subtitle;

  /// [GlassButton]s, right aligned.
  final List<Widget> actions;

  final List<Widget>? children;
  final List<Widget>? slivers;

  /// Fills the rest of the screen instead of [children] (loading, empty).
  final Widget? fill;

  final Future<void> Function()? onRefresh;

  /// A plain (white) list instead of the grouped (gray) background.
  final bool plain;

  @override
  State<LargeTitleScaffold> createState() => _LargeTitleScaffoldState();
}

class _LargeTitleScaffoldState extends State<LargeTitleScaffold> {
  Widget _refreshable({required double edge, required Widget child}) {
    final refresh = widget.onRefresh;
    if (refresh == null) return child;
    return RefreshIndicator.adaptive(edgeOffset: edge, onRefresh: refresh, child: child);
  }

  final _scroll = ScrollController();
  final _collapsed = ValueNotifier(false);

  @override
  void initState() {
    super.initState();
    _scroll.addListener(() => _collapsed.value = _scroll.hasClients && _scroll.offset > 36);
  }

  @override
  void dispose() {
    _scroll.dispose();
    _collapsed.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final pad = MediaQuery.paddingOf(context);
    final background = widget.plain ? Tokens.plain : Tokens.bg;
    const bar = 52.0;
    return AnnotatedRegion<SystemUiOverlayStyle>(
      value: Tokens.dark ? SystemUiOverlayStyle.light : SystemUiOverlayStyle.dark,
      child: Scaffold(
        backgroundColor: background,
        body: Stack(
          children: [
            _refreshable(
              edge: pad.top + bar,
              child: CustomScrollView(
              controller: _scroll,
              physics: const AlwaysScrollableScrollPhysics(parent: BouncingScrollPhysics()),
              slivers: [
                SliverToBoxAdapter(child: SizedBox(height: pad.top + bar)),
                SliverToBoxAdapter(
                  child: Padding(
                    padding: const EdgeInsets.fromLTRB(20, 2, 20, 0),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(children: [
                          Flexible(child: Text(widget.title, style: Tokens.title(size: 34))),
                          if (widget.subtitle != null) InfoIcon(widget.subtitle!, label: widget.title),
                        ]),
                      ],
                    ),
                  ),
                ),
                if (widget.fill != null)
                  SliverFillRemaining(hasScrollBody: false, child: Padding(padding: EdgeInsets.only(bottom: pad.bottom), child: widget.fill))
                else ...[
                  ...?widget.slivers,
                  if (widget.children != null) SliverList(delegate: SliverChildListDelegate(widget.children!)),
                  SliverToBoxAdapter(child: SizedBox(height: listBottom(context))),
                ],
              ],
            ),
            ),
            Positioned(
              left: 0,
              right: 0,
              top: 0,
              child: ValueListenableBuilder<bool>(
                valueListenable: _collapsed,
                builder: (context, collapsed, _) => Stack(
                  children: [
                    Positioned.fill(
                      child: AnimatedOpacity(
                        opacity: collapsed ? 1 : 0,
                        duration: const Duration(milliseconds: 160),
                        child: ClipRect(
                          child: BackdropFilter(
                            filter: ImageFilter.blur(sigmaX: 20, sigmaY: 20),
                            child: Container(
                              decoration: BoxDecoration(
                                color: background.withValues(alpha: 0.72),
                                border: Border(bottom: BorderSide(color: Tokens.sep, width: 0.5)),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                    Padding(
                      padding: EdgeInsets.fromLTRB(16, pad.top + 4, 16, 4),
                      child: SizedBox(
                        height: bar - 8,
                        child: Stack(
                          alignment: Alignment.center,
                          children: [
                            AnimatedOpacity(
                              opacity: collapsed ? 1 : 0,
                              duration: const Duration(milliseconds: 160),
                              child: Text(widget.title, style: TextStyle(fontSize: 17, fontWeight: FontWeight.w600, color: Tokens.label)),
                            ),
                            // Pushed (Settings from More): a way back.
                            if (ModalRoute.of(context)?.canPop ?? false)
                              Align(
                                alignment: Alignment.centerLeft,
                                child: GlassButton(tooltip: 'Back', icon: CupertinoIcons.chevron_back, onPressed: () => Navigator.of(context).maybePop()),
                              ),
                            Align(
                              alignment: Alignment.centerRight,
                              child: Row(mainAxisSize: MainAxisSize.min, children: [
                                for (final (i, a) in widget.actions.indexed) ...[if (i > 0) const SizedBox(width: 10), a],
                              ]),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Bottom space for the end of a scrolling list. Under the floating tab bar
/// [TabScaffold] (extendBody) folds the bar's height into the padding;
/// elsewhere it is the home-indicator inset. Plus a small margin.
double listBottom(BuildContext context, [double margin = 24]) => MediaQuery.paddingOf(context).bottom + margin;

/// The main tabs' frame: [banners] on top, [child] below them and the floating
/// [tabBar] over the page. The page runs on under the bar and sees the bar's
/// height as its bottom padding (see [listBottom]).
class TabScaffold extends StatelessWidget {
  const TabScaffold({super.key, required this.child, required this.tabBar, this.banners = const [], this.bannerVisible = false, this.background});

  final Widget child;
  final Widget tabBar;
  final List<Widget> banners;

  /// A banner covers the status bar, so the page drops its top inset.
  final bool bannerVisible;
  final Color? background;

  @override
  Widget build(BuildContext context) => Scaffold(
        extendBody: true,
        backgroundColor: background,
        // The padding must be read below the Scaffold: only there does it
        // carry the tab bar's height. Read from the context above, it would
        // put back the bare home-indicator inset and the bar would cover the
        // end of every list.
        body: Builder(
          builder: (body) => Column(children: [
            ...banners,
            Expanded(child: MediaQuery.removePadding(context: body, removeTop: bannerVisible, child: child)),
          ]),
        ),
        bottomNavigationBar: tabBar,
      );
}

/// One item of [GlassTabBar].
class GlassTab {
  const GlassTab(this.icon, this.label, {this.badge = 0, this.dot = false});

  final IconData icon;
  final String label;
  final int badge;

  /// Something unread without a count (the Team tab, B8): a small dot when [badge] is 0.
  final bool dot;
}

/// The floating Liquid Glass tab bar, over a fade of the page behind it.
class GlassTabBar extends StatelessWidget {
  const GlassTabBar({super.key, required this.tabs, required this.current, required this.onTap, this.fade});

  final List<GlassTab> tabs;
  final int current;
  final ValueChanged<int> onTap;
  final Color? fade;

  @override
  Widget build(BuildContext context) {
    final bottom = MediaQuery.paddingOf(context).bottom;
    final color = fade ?? Tokens.bg;
    return Container(
      padding: EdgeInsets.fromLTRB(16, 22, 16, bottom > 0 ? bottom : 12),
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.bottomCenter,
          end: Alignment.topCenter,
          colors: [color, color, color.withValues(alpha: 0)],
          stops: const [0, 0.4, 1],
        ),
      ),
      child: Glass(
        radius: 31,
        padding: const EdgeInsets.all(4),
        child: SizedBox(
          height: 54,
          child: Row(
            children: [
              for (final (i, t) in tabs.indexed)
                Expanded(
                  child: Semantics(
                    selected: i == current,
                    button: true,
                    label: t.label,
                    child: GestureDetector(
                      behavior: HitTestBehavior.opaque,
                      onTap: () {
                        HapticFeedback.selectionClick();
                        onTap(i);
                      },
                      child: AnimatedContainer(
                        duration: const Duration(milliseconds: 180),
                        decoration: BoxDecoration(color: i == current ? Tokens.glassSel : Colors.transparent, borderRadius: BorderRadius.circular(27)),
                        child: _TabFace(tab: t, selected: i == current),
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _TabFace extends StatelessWidget {
  const _TabFace({required this.tab, required this.selected});

  final GlassTab tab;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final color = selected ? Tokens.accent : Tokens.label;
    return Column(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        Badge(
          isLabelVisible: tab.badge > 0 || tab.dot,
          smallSize: 8,
          label: tab.badge > 0 ? Text('${tab.badge}') : null,
          backgroundColor: Tokens.red,
          textColor: Colors.white,
          child: Icon(tab.icon, size: 24, color: color),
        ),
        const SizedBox(height: 2),
        ExcludeSemantics(child: Text(tab.label, style: TextStyle(fontSize: 10, fontWeight: FontWeight.w600, color: color))),
      ],
    );
  }
}

/// Header above an inset group: semibold title with an optional "See All".
class GroupHeader extends StatelessWidget {
  const GroupHeader(this.title, {super.key, this.action, this.onAction, this.caps = false, this.count, this.info});

  final String title;
  final String? action;
  final VoidCallback? onAction;

  /// Settings style: small uppercase gray ("THIS PC").
  final bool caps;

  /// A gray count after the title ("Running 2").
  final int? count;

  /// Explanation behind an (i) after the title.
  final String? info;

  @override
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.fromLTRB(32, caps ? 24 : 22, 24, 8),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Expanded(
              child: caps
                  ? Text(title.toUpperCase(), style: Tokens.eyebrow())
                  : Text.rich(TextSpan(
                      text: title,
                      style: TextStyle(fontSize: 17, fontWeight: FontWeight.w600, color: Tokens.label),
                      children: [if (count != null) TextSpan(text: '  $count', style: TextStyle(fontWeight: FontWeight.w500, color: Tokens.label2))],
                    )),
            ),
            if (info != null) InfoIcon(info!, label: title),
            if (action != null)
              GestureDetector(
                onTap: onAction,
                child: Padding(padding: const EdgeInsets.only(left: 12), child: Text(action!, style: TextStyle(fontSize: 15, color: Tokens.accent))),
              ),
          ],
        ),
      );
}

/// Inset grouped list: rounded cell surface with hairlines between rows.
class InsetGroup extends StatelessWidget {
  const InsetGroup({super.key, required this.children, this.dividerIndent = 16, this.margin = const EdgeInsets.symmetric(horizontal: 16), this.padding});

  final List<Widget> children;
  final double dividerIndent;
  final EdgeInsets margin;
  final EdgeInsets? padding;

  @override
  Widget build(BuildContext context) => Padding(
        padding: margin,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(Tokens.cardRadius),
          child: Material(
            color: Tokens.cell,
            child: Padding(
              padding: padding ?? EdgeInsets.zero,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final (i, c) in children.indexed) ...[
                    if (i > 0) Divider(height: 0.5, thickness: 0.5, indent: dividerIndent, color: Tokens.sep),
                    c,
                  ],
                ],
              ),
            ),
          ),
        ),
      );
}

/// A row in an [InsetGroup].
class GroupRow extends StatelessWidget {
  const GroupRow({
    super.key,
    required this.title,
    this.leading,
    this.subtitle,
    this.trailing,
    this.value,
    this.onTap,
    this.chevron,
    this.titleStyle,
    this.titleLines = 1,
    this.padding = const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
  });

  final String title;
  final Widget? leading;
  final String? subtitle;
  final Widget? trailing;

  /// Gray value before the chevron ("1").
  final String? value;
  final VoidCallback? onTap;

  /// Defaults to shown when the row is tappable and has no [trailing].
  final bool? chevron;
  final TextStyle? titleStyle;
  final int titleLines;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) {
    final showChevron = chevron ?? (onTap != null && trailing == null);
    return InkWell(
      onTap: onTap,
      child: Padding(
        padding: padding,
        child: Row(
          children: [
            if (leading != null) ...[leading!, const SizedBox(width: 14)],
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(title, maxLines: titleLines, overflow: TextOverflow.ellipsis, style: titleStyle ?? TextStyle(fontSize: 17, color: Tokens.label)),
                  if (subtitle != null && subtitle!.isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.only(top: 2),
                      child: Text(subtitle!, maxLines: 2, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 13, color: Tokens.label2)),
                    ),
                ],
              ),
            ),
            if (value != null) Padding(padding: const EdgeInsets.only(left: 8), child: Text(value!, style: TextStyle(fontSize: 17, color: Tokens.label2))),
            if (trailing != null) ...[const SizedBox(width: 8), trailing!],
            if (showChevron) const Padding(padding: EdgeInsets.only(left: 6), child: Chevron()),
          ],
        ),
      ),
    );
  }
}

class Chevron extends StatelessWidget {
  const Chevron({super.key});

  @override
  Widget build(BuildContext context) => Icon(CupertinoIcons.chevron_forward, size: 17, color: Tokens.label3);
}

/// Settings-style icon: white glyph on a rounded colored square.
class IconTile extends StatelessWidget {
  const IconTile(this.icon, {super.key, this.color, this.size = 30});

  final IconData icon;
  final Color? color;
  final double size;

  @override
  Widget build(BuildContext context) => Container(
        width: size,
        height: size,
        decoration: BoxDecoration(color: color ?? Tokens.gray, borderRadius: BorderRadius.circular(size * 0.27)),
        child: Icon(icon, size: size * 0.6, color: Colors.white),
      );
}

/// Letter on the accent tint (projects) in a rounded square.
class LetterTile extends StatelessWidget {
  const LetterTile(this.letter, {super.key, this.size = 40});

  final String letter;
  final double size;

  @override
  Widget build(BuildContext context) => Container(
        width: size,
        height: size,
        alignment: Alignment.center,
        decoration: BoxDecoration(color: Tokens.accentTint, borderRadius: BorderRadius.circular(size * 0.275)),
        child: Text(letter, style: TextStyle(fontSize: size * 0.45, fontWeight: FontWeight.w700, color: Tokens.accent)),
      );
}

/// iOS segmented control in the iOS 26 pill shape. Scrolls sideways when the
/// segments do not fit ([scrollable]).
class PillSegments<T> extends StatelessWidget {
  const PillSegments({super.key, required this.segments, required this.selected, required this.onChanged, this.scrollable = false});

  final List<(T, String)> segments;
  final T selected;
  final ValueChanged<T> onChanged;
  final bool scrollable;

  @override
  Widget build(BuildContext context) {
    Widget seg((T, String) s) {
      final on = s.$1 == selected;
      final face = AnimatedContainer(
        duration: const Duration(milliseconds: 180),
        padding: EdgeInsets.symmetric(horizontal: scrollable ? 14 : 8),
        alignment: Alignment.center,
        decoration: BoxDecoration(
          color: on ? Tokens.segSel : Colors.transparent,
          borderRadius: BorderRadius.circular(16),
          boxShadow: on ? const [BoxShadow(color: Color(0x1F000000), blurRadius: 6, offset: Offset(0, 2))] : null,
        ),
        child: Text(s.$2, maxLines: 1, style: TextStyle(fontSize: 14, fontWeight: on ? FontWeight.w600 : FontWeight.w500, color: Tokens.label)),
      );
      return Semantics(
        selected: on,
        button: true,
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: () {
            if (!on) HapticFeedback.selectionClick();
            onChanged(s.$1);
          },
          child: face,
        ),
      );
    }

    return Container(
      height: 36,
      padding: const EdgeInsets.all(2),
      decoration: BoxDecoration(color: Tokens.fill, borderRadius: BorderRadius.circular(18)),
      child: scrollable
          ? ListView(scrollDirection: Axis.horizontal, children: [for (final s in segments) seg(s)])
          : Row(children: [for (final s in segments) Expanded(child: seg(s))]),
    );
  }
}
