import 'dart:ui';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../connection/connection_banner.dart';
import 'session_screen_rules.dart';

// The chrome of the session screen (design "Aurelius sessiescherm", Nov 2026),
// shared by a work session and a ticket: the conversation runs under a glass
// head (back, title, kind pill, "…") and under a floating composer, and the
// "…" opens a glass Properties sheet.

/// How far the conversation starts below the top: the head's height.
double sessionHeadInset(BuildContext context) => MediaQuery.paddingOf(context).top + _headHeight;

/// The head's row: the 44 pt buttons, or the title and its pill.
const _headHeight = 76.0;

/// The screen: [body] fills it (it scrolls under the head), [head] floats on top.
class SessionScaffold extends StatelessWidget {
  const SessionScaffold({super.key, required this.head, required this.body});

  final Widget head;
  final Widget body;

  @override
  Widget build(BuildContext context) => Scaffold(
        backgroundColor: Tokens.bg,
        body: Stack(children: [
          Positioned.fill(child: body),
          Positioned(top: 0, left: 0, right: 0, child: head),
        ]),
      );
}

/// A 44 pt glass circle with one glyph (Back, Properties).
class GlassCircle extends StatelessWidget {
  const GlassCircle({super.key, required this.icon, required this.label, required this.onPressed, this.size = 44, this.iconSize = 20});

  final IconData icon;
  final String label;
  final VoidCallback? onPressed;
  final double size;
  final double iconSize;

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: label,
        child: Glass(
          radius: size / 2,
          child: SizedBox.square(
            dimension: size,
            child: IconButton(tooltip: label, onPressed: onPressed, padding: EdgeInsets.zero, icon: Icon(icon, size: iconSize, color: Tokens.label)),
          ),
        ),
      );
}

/// The head: Back, the title with its kind pill (and [detail] beside it), and
/// "…" ([onMore]). The page under it fades out behind a blur.
class SessionHead extends StatelessWidget {
  const SessionHead({super.key, required this.title, required this.kind, required this.onMore, this.moreLabel = 'Session properties', this.detail, this.detailAccent = false, this.titleKey});

  final String title;
  final SessionKind kind;

  /// "…" was tapped; [anchor] is the button's own context, for a menu under it.
  final void Function(BuildContext anchor)? onMore;
  final String moreLabel;

  /// A quiet line beside the pill ("#70 · Review").
  final String? detail;

  /// [detail] is the board member's to act on: in the accent.
  final bool detailAccent;
  final Key? titleKey;

  @override
  Widget build(BuildContext context) {
    final top = MediaQuery.paddingOf(context).top;
    final bg = Tokens.bg;
    return Stack(children: [
      // The fade: the blur and the page colour give way towards the bottom.
      Positioned.fill(
        bottom: -32,
        child: IgnorePointer(
          child: ShaderMask(
            shaderCallback: (rect) => const LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [Colors.black, Colors.black, Colors.transparent], stops: [0, 0.65, 1]).createShader(rect),
            blendMode: BlendMode.dstIn,
            child: ClipRect(
              child: BackdropFilter(
                filter: ImageFilter.blur(sigmaX: 24, sigmaY: 24),
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    gradient: LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [bg.withValues(alpha: 0.92), bg.withValues(alpha: 0.72), bg.withValues(alpha: 0)], stops: const [0, 0.65, 1]),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
      Padding(
        padding: EdgeInsets.fromLTRB(16, top + 10, 16, 12),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            GlassCircle(key: const ValueKey('session-back'), icon: CupertinoIcons.back, label: 'Back', onPressed: () => Navigator.maybePop(context)),
            const SizedBox(width: 10),
            Expanded(
              child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text(title, key: titleKey, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 16, height: 21 / 16, fontWeight: FontWeight.w600, color: Tokens.label)),
                const SizedBox(height: 5),
                Row(children: [
                  KindPill(kind: kind),
                  if (detail case final detail?) ...[
                    const SizedBox(width: 8),
                    Flexible(
                      child: Text(
                        detail,
                        key: const ValueKey('session-head-detail'),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(fontSize: 12.5, fontWeight: detailAccent ? FontWeight.w600 : FontWeight.w400, color: detailAccent ? Tokens.accentText : Tokens.label2),
                      ),
                    ),
                  ],
                ]),
              ]),
            ),
            const SizedBox(width: 10),
            Builder(
              builder: (anchor) => GlassCircle(key: const ValueKey('session-more'), icon: CupertinoIcons.ellipsis, label: moreLabel, onPressed: onMore == null ? null : () => onMore!(anchor)),
            ),
          ]),
          // Offline or read-only: said under the head, in a rounded card.
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: ClipRRect(borderRadius: BorderRadius.circular(14), child: MediaQuery.removePadding(context: context, removeTop: true, child: const ConnectionBanner())),
          ),
        ]),
      ),
    ]);
  }
}

/// "Session" in green or "Ticket" in blue, under the title.
class KindPill extends StatelessWidget {
  const KindPill({super.key, required this.kind});

  final SessionKind kind;

  @override
  Widget build(BuildContext context) => Container(
        key: ValueKey('session-kind-${kind.name}'),
        height: 24,
        padding: const EdgeInsets.symmetric(horizontal: 10),
        alignment: Alignment.center,
        decoration: BoxDecoration(color: kind == SessionKind.ticket ? Tokens.ticketBlue : Tokens.accent, borderRadius: BorderRadius.circular(999)),
        child: Text(kind.label, style: TextStyle(fontSize: 12, fontWeight: FontWeight.w600, color: Tokens.dark && kind == SessionKind.session ? Tokens.accentInk : Colors.white)),
      );
}

/// "● Waiting for you" (amber) or "● Working" (green) in the Properties sheet.
class StatusChip extends StatelessWidget {
  const StatusChip({super.key, required this.tone, required this.text});

  final StatusTone tone;
  final String text;

  @override
  Widget build(BuildContext context) {
    final working = tone == StatusTone.working;
    return Container(
      height: 22,
      padding: const EdgeInsets.symmetric(horizontal: 9),
      decoration: BoxDecoration(color: working ? Tokens.accentTint : Tokens.waitTint, borderRadius: BorderRadius.circular(999)),
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        Container(width: 6, height: 6, decoration: BoxDecoration(color: working ? Tokens.accent : Tokens.waitDot, shape: BoxShape.circle)),
        const SizedBox(width: 5),
        Flexible(child: Text(text, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 12, fontWeight: FontWeight.w600, color: working ? Tokens.accentDeep : Tokens.waitText))),
      ]),
    );
  }
}

/// A row of the Properties sheet's list under the grid: a count and a chevron.
class PropertiesLink {
  const PropertiesLink(this.label, this.count, this.onTap);

  final String label;
  final int count;
  final VoidCallback onTap;
}

/// The glass Properties sheet: the [rows] as a label/value grid, then the
/// [links] (Files written, Guidelines in this run). [body] replaces both for
/// a list sheet (the files themselves).
Future<void> showPropertiesSheet(BuildContext context, {String title = 'Properties', List<(String, Widget)> rows = const [], List<PropertiesLink> links = const [], Widget? body}) =>
    showModalBottomSheet<void>(
      context: context,
      useRootNavigator: true,
      isScrollControlled: true,
      showDragHandle: false,
      backgroundColor: Colors.transparent,
      elevation: 0,
      barrierColor: const Color(0x4718181B),
      builder: (sheet) => _PropertiesSheet(title: title, rows: rows, links: links, body: body),
    );

class _PropertiesSheet extends StatelessWidget {
  const _PropertiesSheet({required this.title, required this.rows, required this.links, this.body});

  final String title;
  final List<(String, Widget)> rows;
  final List<PropertiesLink> links;
  final Widget? body;

  @override
  Widget build(BuildContext context) {
    final card = BoxDecoration(color: Tokens.cell, border: Border.all(color: Tokens.sep), borderRadius: BorderRadius.circular(18));
    final bottom = MediaQuery.paddingOf(context).bottom;
    return Padding(
      padding: EdgeInsets.fromLTRB(8, MediaQuery.paddingOf(context).top + 24, 8, 8 + bottom / 2),
      child: Glass(
        key: const ValueKey('properties-sheet'),
        radius: 34,
        tint: Tokens.glass.withValues(alpha: Tokens.dark ? 0.85 : 0.82),
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.8),
          child: Material(
            type: MaterialType.transparency,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(16, 8, 16, 20),
              child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                Center(child: Container(width: 36, height: 5, decoration: BoxDecoration(color: Tokens.sepOpaque, borderRadius: BorderRadius.circular(999)))),
                const SizedBox(height: 16),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 4),
                  child: Row(children: [
                    Expanded(child: Text(title, style: TextStyle(fontSize: 17, height: 22 / 17, fontWeight: FontWeight.w600, color: Tokens.label))),
                    GlassCircle(key: const ValueKey('properties-close'), icon: Icons.close, label: 'Close', size: 36, iconSize: 16, onPressed: () => Navigator.pop(context)),
                  ]),
                ),
                const SizedBox(height: 16),
                Flexible(
                  child: SingleChildScrollView(
                    child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                      if (body case final body?) Container(decoration: card, clipBehavior: Clip.antiAlias, child: body),
                      if (rows.isNotEmpty)
                        Container(
                          decoration: card,
                          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
                          child: Column(children: [
                            for (final (i, (label, value)) in rows.indexed)
                              Padding(
                                padding: EdgeInsets.only(top: i == 0 ? 0 : 12),
                                child: Row(crossAxisAlignment: CrossAxisAlignment.center, children: [
                                  SizedBox(width: 96, child: Text(label, style: TextStyle(fontSize: 14, height: 20 / 14, color: Tokens.label2))),
                                  const SizedBox(width: 12),
                                  Expanded(
                                    child: DefaultTextStyle.merge(
                                      style: TextStyle(fontSize: 14, height: 20 / 14, color: Tokens.label),
                                      child: Align(alignment: Alignment.centerLeft, child: value),
                                    ),
                                  ),
                                ]),
                              ),
                          ]),
                        ),
                      if (links.isNotEmpty) ...[
                        const SizedBox(height: 16),
                        Container(
                          decoration: card,
                          clipBehavior: Clip.antiAlias,
                          child: Column(children: [
                            for (final (i, link) in links.indexed)
                              InkWell(
                                key: ValueKey('properties-link-${link.label}'),
                                onTap: link.onTap,
                                child: Container(
                                  padding: const EdgeInsets.fromLTRB(16, 13, 14, 13),
                                  decoration: BoxDecoration(border: i == 0 ? null : Border(top: BorderSide(color: Tokens.sep))),
                                  child: Row(children: [
                                    Expanded(child: Text(link.label, style: TextStyle(fontSize: 15, color: Tokens.label))),
                                    Text('${link.count}', style: TextStyle(fontSize: 14, color: Tokens.label2, fontFeatures: const [FontFeature.tabularFigures()])),
                                    const SizedBox(width: 10),
                                    Icon(CupertinoIcons.chevron_right, size: 14, color: Tokens.label2),
                                  ]),
                                ),
                              ),
                          ]),
                        ),
                      ],
                    ]),
                  ),
                ),
              ]),
            ),
          ),
        ),
      ),
    );
  }
}

/// A plain list for a sheet's [showPropertiesSheet] body: one line per entry,
/// with an optional quiet trailing text (a guideline's version).
class SheetList extends StatelessWidget {
  const SheetList({super.key, required this.entries});

  final List<(String, String?)> entries;

  @override
  Widget build(BuildContext context) => Column(children: [
        for (final (i, (text, trailing)) in entries.indexed)
          Container(
            padding: const EdgeInsets.fromLTRB(16, 12, 14, 12),
            decoration: BoxDecoration(border: i == 0 ? null : Border(top: BorderSide(color: Tokens.sep))),
            child: Row(children: [
              Expanded(child: SelectableText(text, style: TextStyle(fontSize: 14, color: Tokens.label))),
              if (trailing != null) Padding(padding: const EdgeInsets.only(left: 10), child: Text(trailing, style: TextStyle(fontSize: 13, color: Tokens.label2))),
            ]),
          ),
      ]);
}
