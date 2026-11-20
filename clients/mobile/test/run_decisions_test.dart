import 'package:aurelius_mobile/data/run_decisions.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/decision_card.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

MessageView status(String text, {String? author = 'You', Map<String, dynamic>? decision, String update = 'status', String role = 'assistant'}) => MessageView.fromJson({
      'id': 'm1',
      'ordinal': 1,
      'role': role,
      'status': 'complete',
      'text': text,
      'created_at_ms': 0,
      'update': update,
      'author': ?author,
      'decision': ?decision,
      'sources': <dynamic>[],
      'activity': <dynamic>[],
      'files': <dynamic>[],
      'uploads': <dynamic>[],
      'queued': false,
    });

void main() {
  group('a run decision', () {
    test('reads the server field before the text', () {
      final d = decisionOf(status('a sentence this app cannot read', decision: {
        'kind': 'answered',
        'answers': [
          {'question': 'What goes?', 'answer': ' All of it '},
        ],
      }))!;
      expect(d.title, 'You answered');
      expect(d.answers, const [DecisionAnswer('What goes?', 'All of it')]);
    });

    test('reads a plan decision and keeps feedback only when it was sent back or rejected', () {
      final changes = decisionOf(status('x', decision: {'kind': 'plan_changes', 'feedback': 'Smaller steps'}))!;
      expect((changes.verdict, changes.title, changes.feedback), (DecisionVerdict.changes, 'You asked for changes to the plan', 'Smaller steps'));
      final edits = decisionOf(status('x', decision: {'kind': 'plan_approved_edits', 'feedback': 'ignored'}))!;
      expect((edits.verdict, edits.feedback), (DecisionVerdict.approved, null));
    });

    test('shows a kind it does not know as the plain line', () {
      expect(decisionOf(status('Approved the plan.', decision: {'kind': 'plan_paused'})), isNull);
    });

    test('reads an older decision from its text', () {
      final d = decisionOf(status('Answered:\n\n- What happens to the history?\n  **Delete it**\n- What goes?\n  **1-on-1 + Team chat (Recommended)**'))!;
      expect(d.title, 'You answered 2 questions');
      expect(d.answers.last, const DecisionAnswer('What goes?', '1-on-1 + Team chat (Recommended)'));
      expect(decisionOf(status('Rejected the plan: too big'))!.feedback, 'too big');
      expect(decisionOf(status('Approved the plan.'))!.feedback, isNull);
    });

    test('leaves other messages alone', () {
      expect(decisionOf(status('Answered:\n\n- q\n  **a**', role: 'user')), isNull);
      expect(decisionOf(status('Approved the plan.', author: 'Forge')), isNull);
      expect(decisionOf(status('Approved the plan.', update: 'finding')), isNull);
      expect(decisionOf(status('Something else')), isNull);
    });

    test('splits off a trailing (Recommended) only', () {
      expect(recommendedSplit('Keep it (Recommended)'), (text: 'Keep it', recommended: true));
      expect(recommendedSplit('(Recommended) first, then more'), (text: '(Recommended) first, then more', recommended: false));
    });
  });

  testWidgets('the card shows each question with its answer and a quiet Recommended tag', (tester) async {
    final m = status('x', decision: {
      'kind': 'answered',
      'answers': [
        {'question': 'What happens to the history?', 'answer': 'Delete it'},
        {'question': 'What goes?', 'answer': '1-on-1 + Team chat (Recommended)'},
      ],
    });
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: DecisionCard(message: m, decision: decisionOf(m)!))));
    expect(find.text('You answered 2 questions'), findsOneWidget);
    expect(find.text('What goes?'), findsOneWidget);
    expect(find.text('1-on-1 + Team chat'), findsOneWidget);
    expect(find.text('Recommended'), findsOneWidget);
    expect(find.textContaining('**'), findsNothing);
  });
}
