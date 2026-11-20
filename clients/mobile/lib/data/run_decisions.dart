import 'views.dart';

// A board member's answer to a run's questions or decision on its plan, shown as
// a card instead of a grey line (`MessageView.decision`, feature `run_decisions`).
// The same rules as the desktop's `decisionOf` in chat-logic.ts.

/// The way a plan decision went; an answer to questions counts as [approved].
enum DecisionVerdict { approved, changes, rejected }

/// One answered question of a run.
final class DecisionAnswer {
  const DecisionAnswer(this.question, this.answer);

  final String question;
  final String answer;

  @override
  bool operator ==(Object other) => other is DecisionAnswer && other.question == question && other.answer == answer;

  @override
  int get hashCode => Object.hash(question, answer);

  @override
  String toString() => 'DecisionAnswer($question, $answer)';
}

/// What a board member decided, read from a status update of a run.
final class Decision {
  const Decision._({required this.verdict, required this.title, this.answers = const [], this.feedback});

  final DecisionVerdict verdict;

  /// The card's head: "You answered 2 questions", "You approved the plan".
  final String title;

  /// For answered questions: each question with its answer; empty for a plan.
  final List<DecisionAnswer> answers;

  /// For a plan sent back or rejected: what the board member wrote with it.
  final String? feedback;

  bool get isAnswer => answers.isNotEmpty;

  static Decision answered(List<DecisionAnswer> answers) => Decision._(
        verdict: DecisionVerdict.approved,
        title: answers.length == 1 ? 'You answered' : 'You answered ${answers.length} questions',
        answers: answers,
      );
}

typedef _Plan = (DecisionVerdict, String);

const _approved = (DecisionVerdict.approved, 'You approved the plan');
const _approvedEdits = (DecisionVerdict.approved, 'You approved the plan (edits go through)');
const _changes = (DecisionVerdict.changes, 'You asked for changes to the plan');
const _rejected = (DecisionVerdict.rejected, 'You rejected the plan');

/// The plan decisions by `RunDecision.kind` (the server's `run_decision` values).
const Map<String, _Plan> _planKinds = {
  'plan_approved': _approved,
  'plan_approved_edits': _approvedEdits,
  'plan_changes': _changes,
  'plan_rejected': _rejected,
};

/// The decision a status update of a run notes; null for any other message.
Decision? decisionOf(MessageView m) {
  if (m.role != MessageRole.assistant || m.update != 'status' || m.system != null) return null;
  // A kind this app does not know shows as the plain line, never as a guess from its text.
  final d = m.decision;
  if (d != null) return _fromField(d);
  return _fromText(m);
}

Decision? _fromField(RunDecision d) {
  if (d.kind == 'answered') {
    final answers = [
      for (final a in d.answers)
        if (a.question.trim().isNotEmpty && a.answer.trim().isNotEmpty) DecisionAnswer(a.question.trim(), a.answer.trim()),
    ];
    return answers.isEmpty ? null : Decision.answered(answers);
  }
  final plan = _planKinds[d.kind];
  if (plan == null) return null;
  return _plan(plan, d.feedback);
}

Decision _plan(_Plan plan, String? feedback) {
  final (verdict, title) = plan;
  final words = feedback?.trim() ?? '';
  return Decision._(verdict: verdict, title: title, feedback: verdict == DecisionVerdict.approved || words.isEmpty ? null : words);
}

// Decisions noted before the server sent `decision` (or by an older server) only
// have the sentence of approvals.rs, so they are read from it.
const _answeredPrefix = 'Answered:';
final _answerItem = RegExp(r'(?:^|\n)- ([\s\S]+?)\n {2}\*\*([\s\S]*?)\*\*(?=\n- |$)');
const _planLines = <(String, _Plan)>[
  ('Approved the plan; edits go through', _approvedEdits),
  ('Approved the plan', _approved),
  ('Asked for changes to the plan', _changes),
  ('Rejected the plan', _rejected),
];

Decision? _fromText(MessageView m) {
  final author = m.author?.trim() ?? '';
  if (author.isNotEmpty && author != 'You') return null;
  final text = m.text.trim();
  if (text.startsWith(_answeredPrefix)) {
    final body = text.substring(_answeredPrefix.length).trim();
    final answers = [
      for (final x in _answerItem.allMatches(body)) DecisionAnswer((x.group(1) ?? '').trim(), (x.group(2) ?? '').trim()),
    ];
    return answers.isEmpty ? null : Decision.answered(answers);
  }
  for (final (prefix, plan) in _planLines) {
    if (!text.startsWith(prefix)) continue;
    final colon = text.indexOf(':');
    return _plan(plan, colon >= 0 ? text.substring(colon + 1) : null);
  }
  return null;
}

/// An answer cut where Claude's own "(Recommended)" tag starts, so the tag can be shown quieter.
({String text, bool recommended}) recommendedSplit(String answer) {
  const tag = '(Recommended)';
  final at = answer.lastIndexOf(tag);
  if (at < 0 || answer.substring(at + tag.length).trim().isNotEmpty) return (text: answer, recommended: false);
  return (text: answer.substring(0, at).trimRight(), recommended: true);
}
