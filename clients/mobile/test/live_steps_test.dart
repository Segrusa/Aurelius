import 'package:aurelius_mobile/data/protocol/models.dart';
import 'package:aurelius_mobile/features/chat/answer_body.dart';
import 'package:aurelius_mobile/features/chat/live_steps.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/builders.dart';

ActivityView act(String id, String tool, [Object? input, int offset = 0]) =>
    ActivityView(callId: id, input: input, offset: offset, ok: true, tool: tool);

Map<String, Object?> todos(List<(String, String)> rows) => {
      'todos': [for (final (c, s) in rows) {'content': c, 'status': s, 'activeForm': '$c…'}],
    };

QuickReplies? show(List<MessageView> messages, {bool enabled = true, bool unsent = false, bool blocked = false, String draft = '', String? dismissed}) =>
    quickRepliesToShow(messages: messages, enabled: enabled, hasUnsent: unsent, blocked: blocked, draft: draft, dismissedId: dismissed);

MessageView answer({MessageStatus status = MessageStatus.complete, List<ActivityView>? activity}) => messageOf(
      'a1',
      MessageRole.assistant,
      'Done.',
      status: status,
      activity: activity ?? [act('s', suggestRepliesTool, {'question': 'Next?', 'options': ['Yes', 'No', 'Later']})],
    );

void main() {
  group('checklist', () {
    test('parses rows and statuses, skips what is unreadable', () {
      final rows = parseTodos({
        'todos': [
          {'content': 'Write', 'status': 'completed', 'activeForm': 'Writing'},
          {'content': 'Test', 'status': 'in_progress', 'activeForm': 'Testing'},
          {'content': 'Ship', 'status': 'pending'},
          'junk',
        ],
      });
      expect(rows.map((r) => r.status), [TodoStatus.completed, TodoStatus.inProgress, TodoStatus.pending]);
      expect(rows[1].label, 'Testing', reason: 'a row in progress reads in its active form');
      expect(rows[0].label, 'Write');
      expect(parseTodos(null), isEmpty);
      expect(parseTodos({'todos': 'x'}), isEmpty);
    });

    test('only the last TodoWrite shows, and it leaves the step run alone', () {
      final blocks = answerBlocks([
        act('1', 'Bash'),
        act('t1', todoWriteTool, todos([('A', 'pending')])),
        act('2', 'Read'),
        act('t2', todoWriteTool, todos([('A', 'completed')])),
        act('3', 'Edit'),
      ]);
      expect(blocks.whereType<TodoBlock>().map((b) => b.call.callId), ['t2']);
      final steps = blocks.whereType<StepsBlock>().expand((b) => b.steps).map((a) => a.callId);
      expect(steps, ['1', '2', '3']);
    });

    test('an empty checklist and suggest_replies show nothing', () {
      final blocks = answerBlocks([act('t', todoWriteTool, {'todos': []}), act('s', suggestRepliesTool, {'options': ['a', 'b']})]);
      expect(blocks, isEmpty);
    });

    test('the current step is the row in progress of the latest checklist', () {
      expect(currentStep([act('t', todoWriteTool, todos([('A', 'completed'), ('B', 'in_progress')]))]), 'B…');
      expect(currentStep([act('b', 'Bash')]), isNull);
    });

    testWidgets('TodoCard marks rows and folds long lists to the row in progress', (tester) async {
      final rows = [for (var i = 0; i < 10; i++) TodoItem('Step $i', i == 4 ? TodoStatus.inProgress : (i < 4 ? TodoStatus.completed : TodoStatus.pending), 'Doing $i')];
      await tester.pumpWidget(MaterialApp(home: Scaffold(body: TodoCard(todos: rows))));
      expect(find.text('Doing 4'), findsOneWidget);
      expect(find.text('Step 0'), findsNothing);
      expect(find.text('9 more'), findsOneWidget);
      expect(find.text('◐'), findsOneWidget);
      await tester.tap(find.byKey(const Key('todo-fold')));
      await tester.pump();
      expect(find.text('Step 0'), findsOneWidget);
      expect(find.text('✓'), findsNWidgets(4));
      expect(find.text('○'), findsNWidgets(5));
    });

    testWidgets('a short list shows whole and an empty one nothing', (tester) async {
      await tester.pumpWidget(const MaterialApp(home: Scaffold(body: TodoCard(todos: [TodoItem('A', TodoStatus.pending, ''), TodoItem('B', TodoStatus.completed, '')]))));
      expect(find.text('A'), findsOneWidget);
      expect(find.byKey(const Key('todo-fold')), findsNothing);
      await tester.pumpWidget(const MaterialApp(home: Scaffold(body: TodoCard(todos: []))));
      expect(find.text('A'), findsNothing);
    });

    testWidgets('AnswerBody draws the card for the last TodoWrite', (tester) async {
      final m = messageOf('m', MessageRole.assistant, '', status: MessageStatus.partial, activity: [
        act('t1', todoWriteTool, todos([('Old', 'pending')])),
        act('t2', todoWriteTool, todos([('New', 'in_progress')])),
      ]);
      await tester.pumpWidget(MaterialApp(home: Scaffold(body: AnswerBody(message: m))));
      expect(find.byKey(const Key('todo-card')), findsOneWidget);
      expect(find.text('New…'), findsOneWidget);
      expect(find.text('Old'), findsNothing);
    });
  });

  group('quick replies', () {
    test('parses question and at most three options, labels cut at 40', () {
      final r = parseQuickReplies(act('s', suggestRepliesTool, {'question': ' Q ', 'options': ['a', ' ', 'b', 'c', 'd']}), 'm')!;
      expect(r.question, 'Q');
      expect(r.options, ['a', 'b', 'c']);
      expect(parseQuickReplies(act('s', suggestRepliesTool, {'options': ['only']}), 'm'), isNull);
      expect(parseQuickReplies(act('s', suggestRepliesTool, 'x'), 'm'), isNull);
      expect(quickReplyLabel('x' * 60).length, quickReplyLabelMax);
      expect(quickReplyLabel('short'), 'short');
    });

    test('shown under the last complete answer', () {
      final r = show([messageOf('u', MessageRole.user, 'hi'), answer()])!;
      expect(r.options, ['Yes', 'No', 'Later']);
      expect(r.messageId, 'a1');
    });

    test('hidden without the feature, or while anything else is going on', () {
      final base = [answer()];
      expect(show(base, enabled: false), isNull);
      expect(show(base, draft: 'typing'), isNull);
      expect(show(base, draft: '  '), isNotNull, reason: 'blank space is no draft');
      expect(show(base, unsent: true), isNull);
      expect(show(base, blocked: true), isNull);
      expect(show(base, dismissed: 'a1'), isNull);
      expect(show([answer(status: MessageStatus.partial)]), isNull);
      expect(show([answer(status: MessageStatus.failed)]), isNull);
      expect(show([...base, messageOf('u2', MessageRole.user, 'again')]), isNull, reason: 'a new message');
      expect(show([...base, answer(activity: const [])]), isNull, reason: 'the new answer has none');
      expect(show([answer(activity: [act('s', suggestRepliesTool, {'options': ['a', 'b']}), act('p', 'claude.ExitPlanMode', {'plan': 'x'})])]), isNull, reason: 'a plan');
      expect(show([messageOf('t', MessageRole.assistant, 'x', update: 'brief', activity: [act('s', suggestRepliesTool, {'options': ['a', 'b']})])]), isNull);
      expect(show(const []), isNull);
    });
  });
}
