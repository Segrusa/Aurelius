import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/views.dart';
import '../connection/connection_banner.dart';
import '../../core/widgets/ios.dart';

/// Claude asks the user something (AskUserQuestion): pick an option per
/// question (or several when it allows that), or write your own answer.
/// ⌘/Ctrl+Enter sends, ⌘/Ctrl+Backspace skips (Claude then decides itself).
class QuestionPage extends StatefulWidget {
  const QuestionPage({super.key, required this.approval, required this.busy, required this.minutes, required this.onSkip, required this.onAnswer});

  final ApprovalView approval;
  final bool busy;
  final int minutes;
  final VoidCallback onSkip;
  final ValueChanged<Map<String, String>> onAnswer;

  @override
  State<QuestionPage> createState() => _QuestionPageState();
}

class _QuestionPageState extends State<QuestionPage> {
  /// Chosen option indices per question.
  final Map<int, Set<int>> _chosen = {};

  /// Own answers per question (replace the options when not empty).
  final Map<int, TextEditingController> _own = {};

  @override
  void dispose() {
    for (final c in _own.values) {
      c.dispose();
    }
    super.dispose();
  }

  TextEditingController _ownFor(int i) => _own.putIfAbsent(i, () => TextEditingController()..addListener(() => setState(() {})));

  /// The answers, or `null` while a question is unanswered.
  Map<String, String>? _answers(List<Question> questions) {
    final out = <String, String>{};
    for (final (i, q) in questions.indexed) {
      final own = _own[i]?.text.trim() ?? '';
      final chosen = (_chosen[i] ?? const <int>{}).toList()..sort();
      final answer = own.isNotEmpty ? own : chosen.where((n) => n < q.options.length).map((n) => q.options[n].label).join(', ');
      if (answer.isEmpty) return null;
      out[q.text] = answer;
    }
    return out;
  }

  void _choose(Question q, int i, int n) {
    setState(() {
      _own[i]?.clear();
      final set = _chosen.putIfAbsent(i, () => <int>{});
      if (q.multi) {
        set.contains(n) ? set.remove(n) : set.add(n);
      } else {
        set
          ..clear()
          ..add(n);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final questions = widget.approval.questions;
    final answers = _answers(questions);
    final canSend = !widget.busy && widget.minutes > 0 && answers != null;
    void send() {
      if (canSend) widget.onAnswer(answers);
    }

    void skip() {
      if (!widget.busy) widget.onSkip();
    }

    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.enter, meta: true): send,
        const SingleActivator(LogicalKeyboardKey.enter, control: true): send,
        const SingleActivator(LogicalKeyboardKey.backspace, meta: true): skip,
        const SingleActivator(LogicalKeyboardKey.backspace, control: true): skip,
      },
      child: Focus(
        autofocus: true,
        child: Scaffold(
          appBar: GlassAppBar(title: const Text('Question')),
          body: Column(
            children: [
              const ConnectionBanner(),
              Expanded(
                child: ListView(
                  padding: const EdgeInsets.fromLTRB(16, 8, 16, 24),
                  children: [
                    Text(widget.approval.title, style: Tokens.title(size: 24)),
                    const SizedBox(height: 4),
                    Text(
                      widget.minutes <= 0 ? 'Expired' : 'Expires in ${widget.minutes} min; then Claude continues on its own judgment.',
                      style: TextStyle(color: Tokens.muted, fontSize: 12.5),
                    ),
                    for (final (i, q) in questions.indexed) ...[
                      const SizedBox(height: 16),
                      SurfaceCard(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            if (q.header.isNotEmpty || q.multi)
                              Padding(
                                padding: const EdgeInsets.only(bottom: 6),
                                child: Text(
                                  [if (q.header.isNotEmpty) q.header.toUpperCase(), if (q.multi) 'CHOOSE ONE OR MORE'].join(' · '),
                                  style: Tokens.eyebrow(),
                                ),
                              ),
                            Text(q.text, style: const TextStyle(fontSize: 16, height: 1.4)),
                            const SizedBox(height: 8),
                            for (final (n, o) in q.options.indexed)
                              InkWell(
                                key: Key('option-$i-$n'),
                                borderRadius: BorderRadius.circular(Tokens.radius),
                                onTap: widget.busy ? null : () => _choose(q, i, n),
                                child: Padding(
                                  padding: const EdgeInsets.symmetric(vertical: 8),
                                  child: Row(
                                    crossAxisAlignment: CrossAxisAlignment.start,
                                    children: [
                                      Icon(
                                        _icon(q.multi, (_chosen[i] ?? const <int>{}).contains(n) && (_own[i]?.text.trim().isEmpty ?? true)),
                                        size: 20,
                                        color: Tokens.accentText,
                                      ),
                                      const SizedBox(width: 10),
                                      Expanded(
                                        child: Column(
                                          crossAxisAlignment: CrossAxisAlignment.start,
                                          children: [
                                            Text(o.label, style: const TextStyle(fontWeight: FontWeight.w600)),
                                            if (o.description.isNotEmpty) Text(o.description, style: TextStyle(color: Tokens.text2, fontSize: 13)),
                                          ],
                                        ),
                                      ),
                                    ],
                                  ),
                                ),
                              ),
                            const SizedBox(height: 4),
                            TextField(
                              key: Key('own-$i'),
                              controller: _ownFor(i),
                              enabled: !widget.busy,
                              minLines: 1,
                              maxLines: 4,
                              decoration: const InputDecoration(hintText: 'Or answer in your own words'),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ],
                ),
              ),
              SafeArea(
                top: false,
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(16, 8, 16, 12),
                  child: Row(
                    children: [
                      Expanded(child: OutlinedButton(onPressed: widget.busy ? null : skip, child: const Text('Skip'))),
                      const SizedBox(width: 12),
                      Expanded(
                        child: FilledButton(
                          onPressed: canSend ? send : null,
                          child: widget.busy
                              ? SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2, color: Tokens.bg))
                              : const Text('Send answer'),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  static IconData _icon(bool multi, bool on) => switch ((multi, on)) {
        (true, true) => Icons.check_box,
        (true, false) => Icons.check_box_outline_blank,
        (false, true) => Icons.radio_button_checked,
        (false, false) => Icons.radio_button_unchecked,
      };
}
