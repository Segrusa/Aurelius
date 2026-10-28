import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../data/line_diff.dart';

/// Diff lines in mono, removed red with "−", added green with "+". Long
/// diffs show their first [collapsedLines] lines with "Show all".
class DiffLines extends StatefulWidget {
  const DiffLines({super.key, required this.lines, this.collapsedLines = 14});

  final List<DiffLine> lines;
  final int collapsedLines;

  /// A unified diff text (git): lines starting with + or − coloured, headers muted.
  factory DiffLines.unified(String diff, {Key? key, int collapsedLines = 14}) => DiffLines(
        key: key,
        collapsedLines: collapsedLines,
        lines: [
          for (final l in diff.split('\n'))
            if (l.startsWith('+++') || l.startsWith('---'))
              (op: DiffOp.same, text: l)
            else if (l.startsWith('+'))
              (op: DiffOp.added, text: l.substring(1))
            else if (l.startsWith('-'))
              (op: DiffOp.removed, text: l.substring(1))
            else
              (op: DiffOp.same, text: l.startsWith(' ') ? l.substring(1) : l),
        ],
      );

  @override
  State<DiffLines> createState() => _DiffLinesState();
}

class _DiffLinesState extends State<DiffLines> {
  bool _all = false;

  @override
  Widget build(BuildContext context) {
    final lines = widget.lines;
    final shown = _all || lines.length <= widget.collapsedLines ? lines : lines.take(widget.collapsedLines).toList();
    final style = TextStyle(fontFamily: Tokens.mono, fontSize: 12, height: 1.4, color: Tokens.text2);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          decoration: BoxDecoration(color: Tokens.cell2, borderRadius: BorderRadius.circular(Tokens.radius)),
          padding: const EdgeInsets.symmetric(vertical: 6),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final l in shown)
                Container(
                  color: switch (l.op) {
                    DiffOp.added => Tokens.accentTint,
                    DiffOp.removed => Tokens.red.withValues(alpha: 0.16),
                    DiffOp.same => null,
                  },
                  padding: const EdgeInsets.symmetric(horizontal: 8),
                  child: Text(
                    '${switch (l.op) { DiffOp.added => '+', DiffOp.removed => '−', DiffOp.same => ' ' }} ${l.text}',
                    style: style.copyWith(color: switch (l.op) { DiffOp.added => Tokens.accent, DiffOp.removed => Tokens.red, DiffOp.same => Tokens.text2 }),
                  ),
                ),
              if (lines.isEmpty) Padding(padding: const EdgeInsets.symmetric(horizontal: 8), child: Text('No changes.', style: style)),
            ],
          ),
        ),
        if (shown.length < lines.length)
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton(onPressed: () => setState(() => _all = true), child: Text('Show all ${lines.length} lines')),
          ),
      ],
    );
  }
}
