import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:intl/intl.dart';

import '../theme/tokens.dart';

/// Section label above a list ("NEEDS YOUR ATTENTION"), in the grouped-list
/// header style.
class Eyebrow extends StatelessWidget {
  const Eyebrow(this.text, {super.key, this.padding = const EdgeInsets.fromLTRB(32, 22, 24, 8)});

  final String text;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) => Padding(padding: padding, child: Text(text.toUpperCase(), style: Tokens.eyebrow()));
}

/// The (i) next to a title or setting: the explanation lives behind it, in a
/// small sheet, so the screen itself stays quiet. [label] names what it
/// explains for screen readers ("Info: Load limits").
class InfoIcon extends StatelessWidget {
  const InfoIcon(this.text, {super.key, required this.label});

  final String text;
  final String label;

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: 'Info: $label',
        excludeSemantics: true,
        onTap: () => _show(context),
        child: InkResponse(
          onTap: () => _show(context),
          radius: 22,
          child: SizedBox(width: 44, height: 44, child: Icon(Icons.info_outline, size: 20, color: Tokens.label3)),
        ),
      );

  void _show(BuildContext context) => showModalBottomSheet<void>(
        context: context,
        showDragHandle: true,
        builder: (sheet) => SafeArea(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(24, 0, 24, 20),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(label, style: Tokens.title(size: 18)),
                const SizedBox(height: 8),
                Flexible(child: SingleChildScrollView(child: Text(text, key: const Key('info-text'), style: TextStyle(fontSize: 15, height: 1.4, color: Tokens.label2)))),
              ],
            ),
          ),
        ),
      );
}

/// What an empty list says: a quiet headline, a line of detail, an action.
class EmptyState extends StatelessWidget {
  const EmptyState(this.text, {super.key, this.detail, this.action});

  final String text;
  final String? detail;
  final Widget? action;

  @override
  Widget build(BuildContext context) => Center(
        child: Padding(
          padding: const EdgeInsets.all(32),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(text, textAlign: TextAlign.center, style: TextStyle(fontSize: 20, fontWeight: FontWeight.w600, color: Tokens.label)),
              if (detail != null) ...[
                const SizedBox(height: 6),
                Text(detail!, textAlign: TextAlign.center, style: TextStyle(color: Tokens.label2, fontSize: 15, height: 1.35)),
              ],
              if (action != null) ...[const SizedBox(height: 18), action!],
            ],
          ),
        ),
      );
}

/// An employee's letter in a filled circle; the accent tint while working.
/// A [member] (a board member) gets an outlined circle instead, as on the
/// desktop.
class Monogram extends StatelessWidget {
  const Monogram(this.letter, {super.key, this.size = 36, this.active = false, this.member = false});

  final String letter;
  final double size;
  final bool active;
  final bool member;

  @override
  Widget build(BuildContext context) => Container(
        width: size,
        height: size,
        alignment: Alignment.center,
        decoration: member
            ? BoxDecoration(shape: BoxShape.circle, border: Border.all(color: Tokens.text2))
            : BoxDecoration(shape: BoxShape.circle, color: active ? Tokens.accentTint : Tokens.fill),
        child: Text(letter, style: TextStyle(fontSize: size * 0.43, fontWeight: FontWeight.w600, color: active ? Tokens.accent : (member ? Tokens.text2 : Tokens.label))),
      );
}

/// Small status dot; `pulse` breathes while something is working.
class StatusDot extends StatefulWidget {
  const StatusDot({super.key, this.color, this.pulse = false, this.size = 8});

  final Color? color;
  final bool pulse;
  final double size;

  @override
  State<StatusDot> createState() => _StatusDotState();
}

class _StatusDotState extends State<StatusDot> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(vsync: this, duration: const Duration(milliseconds: 1400));

  @override
  void initState() {
    super.initState();
    if (widget.pulse) _c.repeat(reverse: true);
  }

  @override
  void didUpdateWidget(StatusDot old) {
    super.didUpdateWidget(old);
    if (widget.pulse && !_c.isAnimating) _c.repeat(reverse: true);
    if (!widget.pulse && _c.isAnimating) _c.stop();
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final color = widget.color ?? Tokens.accent;
    return AnimatedBuilder(
      animation: _c,
      builder: (context, _) => Container(
        width: widget.size,
        height: widget.size,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: color.withValues(alpha: widget.pulse ? 0.55 + 0.45 * _c.value : 1),
          boxShadow: widget.pulse ? [BoxShadow(color: color.withValues(alpha: 0.35 * _c.value), blurRadius: 6, spreadRadius: 1)] : null,
        ),
      ),
    );
  }
}

/// "Answer", "Review plan"…: what a conversation in a list waits for from you.
class NeedsYouTag extends StatelessWidget {
  const NeedsYouTag(this.label, {super.key});

  final String label;

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
        decoration: BoxDecoration(color: Tokens.accentTint, borderRadius: BorderRadius.circular(999)),
        child: Text(label, style: TextStyle(color: Tokens.accent, fontSize: 12, fontWeight: FontWeight.w600)),
      );
}

/// The "needs you" card: a cell with an accent edge.
class AttentionCard extends StatelessWidget {
  const AttentionCard({super.key, required this.child, this.onTap});

  final Widget child;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) => Material(
        color: Tokens.cell,
        clipBehavior: Clip.antiAlias,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Tokens.cardRadius), side: BorderSide(color: Tokens.accent.withValues(alpha: 0.45))),
        child: InkWell(onTap: onTap, child: Padding(padding: const EdgeInsets.all(16), child: child)),
      );
}

/// A rounded cell surface.
class SurfaceCard extends StatelessWidget {
  const SurfaceCard({super.key, required this.child, this.onTap, this.padding = const EdgeInsets.all(16), this.selected = false});

  final Widget child;
  final VoidCallback? onTap;
  final EdgeInsets padding;
  final bool selected;

  @override
  Widget build(BuildContext context) => Material(
        color: selected ? Tokens.cell2 : Tokens.cell,
        clipBehavior: Clip.antiAlias,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Tokens.cardRadius)),
        child: InkWell(onTap: onTap, child: Padding(padding: padding, child: child)),
      );
}

class ErrorRetry extends StatelessWidget {
  const ErrorRetry(this.message, {super.key, this.onRetry});

  final String message;
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) => EmptyState(
        'Something went wrong',
        detail: message,
        action: onRetry == null ? null : OutlinedButton(onPressed: onRetry, child: const Text('Try again')),
      );
}

/// Label / value line in detail views.
class InfoRow extends StatelessWidget {
  const InfoRow(this.label, this.value, {super.key, this.mono = false});

  final String label;
  final String value;
  final bool mono;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.symmetric(vertical: 6),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(width: 110, child: Text(label, style: TextStyle(color: Tokens.label2, fontSize: 15))),
            Expanded(
              child: SelectableText(value, style: mono ? TextStyle(fontFamily: Tokens.mono, fontSize: 13, color: Tokens.label) : TextStyle(fontSize: 15, color: Tokens.label)),
            ),
          ],
        ),
      );
}

/// A small copy icon that puts [text] on the clipboard and shows a check
/// with "Copied" for a moment.
class CopyButton extends StatefulWidget {
  const CopyButton({super.key, required this.text, this.fetch, this.label = false, this.tooltip = 'Copy'});

  final String text;

  /// Fetches the text to copy instead of [text] (a shortened tool output is
  /// fetched whole first); null when that failed, so nothing is copied.
  final Future<String?> Function()? fetch;

  /// Also show the word next to the icon.
  final bool label;
  final String tooltip;

  @override
  State<CopyButton> createState() => _CopyButtonState();
}

class _CopyButtonState extends State<CopyButton> {
  bool _done = false;
  Timer? _timer;

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  Future<void> _copy() async {
    final fetch = widget.fetch;
    final text = fetch == null ? widget.text : await fetch();
    if (text == null || !mounted) return;
    await Clipboard.setData(ClipboardData(text: text));
    if (!mounted) return;
    setState(() => _done = true);
    _timer?.cancel();
    _timer = Timer(const Duration(milliseconds: 1500), () {
      if (mounted) setState(() => _done = false);
    });
  }

  @override
  Widget build(BuildContext context) {
    final color = _done ? Tokens.accentText : Tokens.muted;
    return Semantics(
      button: true,
      label: _done ? 'Copied' : widget.tooltip,
      child: InkWell(
        onTap: _copy,
        borderRadius: BorderRadius.circular(8),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 5),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            Icon(_done ? Icons.check : Icons.copy_outlined, size: 15, color: color),
            if (widget.label || _done) ...[
              const SizedBox(width: 4),
              Text(_done ? 'Copied' : widget.tooltip, style: TextStyle(fontSize: 11.5, color: color)),
            ],
          ]),
        ),
      ),
    );
  }
}

/// Show a short message at the bottom.
void toast(BuildContext context, String message) {
  ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(content: Text(message)));
}

abstract final class Fmt {
  static String clock(int ms) => DateFormat.Hm().format(DateTime.fromMillisecondsSinceEpoch(ms));

  /// "just now", "5 min ago", "14:05", "Mon 14:05", "12 Mar".
  static String ago(int ms) {
    final at = DateTime.fromMillisecondsSinceEpoch(ms);
    final diff = DateTime.now().difference(at);
    if (diff.inSeconds < 45) return 'just now';
    if (diff.inMinutes < 60) return '${diff.inMinutes} min ago';
    final now = DateTime.now();
    if (at.year == now.year && at.month == now.month && at.day == now.day) return DateFormat.Hm().format(at);
    if (diff.inDays < 7) return DateFormat('EEE HH:mm').format(at);
    return DateFormat('d MMM').format(at);
  }

  static String tokens(int n) {
    if (n >= 1000000) return '${(n / 1000000).toStringAsFixed(1)}M';
    if (n >= 1000) return '${(n / 1000).toStringAsFixed(n >= 10000 ? 0 : 1)}k';
    return '$n';
  }

  /// How long something runs: "45s", "2m 26s", "1h 03m".
  static String elapsed(int ms) {
    final s = (ms < 0 ? 0 : ms) ~/ 1000;
    if (s < 60) return '${s}s';
    if (s < 3600) return '${s ~/ 60}m ${s % 60}s';
    return '${s ~/ 3600}h ${((s % 3600) ~/ 60).toString().padLeft(2, '0')}m';
  }

  /// "2m 26s · 3.9k tokens", as on the desktop.
  static String progress(int startedAtMs, int outputTokens, {int? nowMs}) {
    final time = elapsed((nowMs ?? DateTime.now().millisecondsSinceEpoch) - startedAtMs);
    return outputTokens == 0 ? time : '$time · ${tokens(outputTokens)} tokens';
  }

  static String bytes(int n) {
    if (n >= 1 << 20) return '${(n / (1 << 20)).toStringAsFixed(1)} MB';
    if (n >= 1 << 10) return '${(n / (1 << 10)).toStringAsFixed(0)} KB';
    return '$n B';
  }

  /// Chat list groups, as on the desktop.
  static String dayGroup(int ms) {
    final at = DateTime.fromMillisecondsSinceEpoch(ms);
    final today = DateUtils.dateOnly(DateTime.now());
    final day = DateUtils.dateOnly(at);
    final days = today.difference(day).inDays;
    if (days <= 0) return 'Today';
    if (days == 1) return 'Yesterday';
    if (days < 7) return 'This week';
    return 'Earlier';
  }
}
