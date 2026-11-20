import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/run_decisions.dart';
import '../../data/views.dart';
import '../markdown/links.dart';

/// What the board member answered or decided on a run, as a card under the
/// question instead of a grey line: a mark, a head with the time, and each
/// question with its answer (or the feedback on a plan). As on the desktop.
class DecisionCard extends StatelessWidget {
  const DecisionCard({super.key, required this.message, required this.decision});

  final MessageView message;
  final Decision decision;

  @override
  Widget build(BuildContext context) {
    final time = Fmt.clock(message.createdAtMs);
    final (markColor, markIcon) = switch (decision.verdict) {
      DecisionVerdict.approved => (Tokens.accent, Icons.check_rounded),
      DecisionVerdict.changes => (Tokens.warning, Icons.edit_outlined),
      DecisionVerdict.rejected => (Tokens.negative, Icons.close_rounded),
    };
    final divider = BoxDecoration(border: Border(top: BorderSide(color: Tokens.border)));
    return Semantics(
      container: true,
      label: '${decision.title} at $time',
      child: Container(
        key: Key('decision-${message.id}'),
        margin: const EdgeInsets.only(top: 14),
        width: double.infinity,
        clipBehavior: Clip.antiAlias,
        decoration: BoxDecoration(color: Tokens.surface, border: Border.all(color: Tokens.border), borderRadius: BorderRadius.circular(10)),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 9, 12, 9),
            child: Row(children: [
              Container(
                width: 20,
                height: 20,
                decoration: BoxDecoration(color: markColor.withValues(alpha: 0.18), shape: BoxShape.circle),
                child: Icon(markIcon, size: 13, color: markColor),
              ),
              const SizedBox(width: 8),
              Expanded(child: Text(decision.title, style: TextStyle(fontSize: 13.5, fontWeight: FontWeight.w600, color: Tokens.text))),
              const SizedBox(width: 8),
              Text(time, style: TextStyle(fontSize: 12, color: Tokens.text2)),
            ]),
          ),
          for (final a in decision.answers)
            Container(
              width: double.infinity,
              decoration: divider,
              padding: const EdgeInsets.fromLTRB(12, 10, 12, 11),
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text(a.question, style: TextStyle(fontSize: 13, height: 1.4, color: Tokens.text2)),
                const SizedBox(height: 6),
                _Answer(answer: a.answer),
              ]),
            ),
          if (decision.feedback != null)
            Container(
              width: double.infinity,
              decoration: divider,
              padding: const EdgeInsets.fromLTRB(12, 8, 12, 10),
              child: MessageMarkdown(data: decision.feedback!, scope: null, selectable: true),
            ),
        ]),
      ),
    );
  }
}

/// An answer behind an accent bar, with Claude's "(Recommended)" as a quiet tag.
class _Answer extends StatelessWidget {
  const _Answer({required this.answer});

  final String answer;

  @override
  Widget build(BuildContext context) {
    final (:text, :recommended) = recommendedSplit(answer);
    return Container(
      padding: const EdgeInsets.only(left: 9),
      decoration: BoxDecoration(border: Border(left: BorderSide(color: Tokens.accent, width: 2))),
      child: Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 8, runSpacing: 4, children: [
        SelectableText(text, style: TextStyle(fontSize: 15, height: 1.4, fontWeight: FontWeight.w500, color: Tokens.text)),
        if (recommended)
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
            decoration: BoxDecoration(color: Tokens.fill, borderRadius: BorderRadius.circular(4)),
            child: Text('Recommended', style: TextStyle(fontSize: 11.5, fontWeight: FontWeight.w500, color: Tokens.text2)),
          ),
      ]),
    );
  }
}
