import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';

/// Messages longer than this many lines of raw Markdown (code blocks count)
/// show only their first lines until the user unfolds them.
const collapseLines = 20;

/// Where a long message is cut: the end of its [collapseLines]th line, and
/// how many lines are hidden after it. `null` for a message that fits.
(int, int)? collapseCut(String text) {
  final trimmed = text.trimRight();
  var at = -1;
  for (var line = 0; line < collapseLines; line++) {
    at = trimmed.indexOf('\n', at + 1);
    if (at < 0) return null;
  }
  return (at, '\n'.allMatches(trimmed.substring(at)).length);
}

final _fence = RegExp(r'^(\s*)(`{3,}|~{3,})(.*)$');

/// The Markdown of [text] before [cut]; a code block the cut falls in is
/// closed, so it still shows as code.
String collapsedMarkdown(String text, int cut) {
  final shown = text.substring(0, cut);
  String? open; // the fence of the code block we are in, with its indent
  for (final line in shown.split('\n')) {
    final m = _fence.firstMatch(line);
    if (m == null) continue;
    final fence = m[2]!;
    if (open == null) {
      // A backtick fence's info string has no backticks.
      if (fence[0] == '`' && m[3]!.contains('`')) continue;
      open = '${m[1]}$fence';
    } else {
      final opener = open.trimLeft();
      if (fence[0] == opener[0] && fence.length >= opener.length && m[3]!.trim().isEmpty) open = null;
    }
  }
  return open == null ? shown : '$shown\n$open';
}

/// Shows [text] cut after [collapseLines] lines with "Show more (N lines)"
/// below, and "Show less" once unfolded. [builder] gets the cut (a UTF-16
/// index into the text) or `null` for the whole message.
///
/// A message that streams is never cut: it is shown whole while it comes in
/// and stays unfolded when it ends, since the user was reading it; a long
/// message loaded from history starts folded.
class Collapsible extends StatefulWidget {
  const Collapsible({super.key, required this.id, required this.text, required this.builder, this.streaming = false, this.alignment = CrossAxisAlignment.start});

  /// The message; its choice is kept for the rest of the session, also when
  /// the list scrolls it away and back.
  final String id;
  final String text;
  final bool streaming;
  final CrossAxisAlignment alignment;
  final Widget Function(BuildContext context, int? cut) builder;

  /// Messages unfolded in this session.
  static final expanded = <String>{};

  @override
  State<Collapsible> createState() => _CollapsibleState();
}

class _CollapsibleState extends State<Collapsible> {
  @override
  Widget build(BuildContext context) {
    if (widget.streaming) Collapsible.expanded.add(widget.id);
    final cut = widget.streaming ? null : collapseCut(widget.text);
    if (cut == null) return widget.builder(context, null);
    final open = Collapsible.expanded.contains(widget.id);
    return Column(
      crossAxisAlignment: widget.alignment,
      children: [
        widget.builder(context, open ? null : cut.$1),
        TextButton.icon(
          key: const Key('collapse-toggle'),
          style: TextButton.styleFrom(visualDensity: VisualDensity.compact, padding: const EdgeInsets.symmetric(horizontal: 6), foregroundColor: Tokens.accentText),
          onPressed: () => setState(() => open ? Collapsible.expanded.remove(widget.id) : Collapsible.expanded.add(widget.id)),
          icon: Icon(open ? Icons.expand_less : Icons.expand_more, size: 18),
          label: Text(open ? 'Show less' : 'Show more (${cut.$2} ${cut.$2 == 1 ? 'line' : 'lines'})'),
        ),
      ],
    );
  }
}
