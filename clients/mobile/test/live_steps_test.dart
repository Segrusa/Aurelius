import 'package:aurelius_mobile/data/protocol/models.dart';
import 'package:aurelius_mobile/features/chat/answer_body.dart';
import 'package:aurelius_mobile/features/chat/live_steps.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/builders.dart';

ActivityView act(String id, String tool, [Object? input, int offset = 0, String? output]) =>
    ActivityView(callId: id, input: input, offset: offset, ok: true, tool: tool, output: output);

/// A TaskCreate as Claude Code 2.1.318 reports it: the number is in the result.
ActivityView create(int n, String subject, [int offset = 0]) =>
    act('c$n', taskCreateTool, {'subject': subject, 'activeForm': '$subject…'}, offset, 'Task #$n created successfully: $subject');

ActivityView update(int n, String status, [int offset = 0]) => act('u$n-$status', taskUpdateTool, {'taskId': '$n', 'status': status}, offset);

List<(String, TodoStatus)> rowsOf(List<TodoItem>? todos) => [for (final t in todos ?? const <TodoItem>[]) (t.content, t.status)];

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
      expect(blocks.whereType<TodoBlock>().map((b) => rowsOf(b.todos)), [[('A', TodoStatus.completed)]]);
      expect(blocks.indexWhere((b) => b is TodoBlock), 1, reason: 'the card stands where the last call was');
      final steps = blocks.whereType<StepsBlock>().expand((b) => b.steps).map((a) => a.callId);
      expect(steps, ['1', '2', '3']);
    });

    test('an empty checklist and suggest_replies show nothing', () {
      final blocks = answerBlocks([act('t', todoWriteTool, {'todos': []}), act('s', suggestRepliesTool, {'options': ['a', 'b']})]);
      expect(blocks, isEmpty);
    });

    test('the current step is the row in progress of the latest checklist', () {
      expect(currentStep(todosAcross([[act('t', todoWriteTool, todos([('A', 'completed'), ('B', 'in_progress')]))]])), 'B…');
      expect(currentStep(todosAcross([[act('b', 'Bash')]])), isNull);
    });

    test('TaskCreate and TaskUpdate in one answer make the card, and are no steps', () {
      final blocks = answerBlocks([create(1, 'A', 0), create(2, 'B', 1), act('r', 'Read', null, 2), update(1, 'in_progress', 3)]);
      expect(blocks.whereType<TodoBlock>().map((b) => rowsOf(b.todos)), [
        [('A', TodoStatus.inProgress), ('B', TodoStatus.pending)],
      ]);
      expect(blocks.whereType<StepsBlock>().expand((b) => b.steps).map((a) => a.callId), ['r']);
    });

    test('an update in a later answer changes the list made in an earlier one', () {
      final todos = todosAcross([
        [create(1, 'A'), create(2, 'B', 1), create(3, 'C', 2)],
        [update(1, 'completed'), update(2, 'in_progress', 1)],
      ]);
      expect(rowsOf(todos), [('A', TodoStatus.completed), ('B', TodoStatus.inProgress), ('C', TodoStatus.pending)]);
      expect(currentStep(todos), 'B…');
    });

    test('steps that run at the same time show the first and how many more', () {
      final two = todosAcross([
        [create(1, 'A'), create(2, 'B', 1), create(3, 'C', 2)],
        [update(1, 'completed'), update(2, 'in_progress', 1), update(3, 'in_progress', 2)],
      ]);
      expect(currentStep(two), 'B… +1');
      final three = todosAcross([[act('t', todoWriteTool, todos([('A', 'in_progress'), ('B', 'pending'), ('C', 'in_progress'), ('D', 'in_progress')]))]]);
      expect(currentStep(three), 'A… +2');
    });

    test('items added in a later answer join the earlier list, deleted ones go', () {
      final todos = todosAcross([
        [create(1, 'A'), create(2, 'B', 1)],
        [update(1, 'completed'), update(2, 'deleted', 1), create(3, 'C', 2)],
      ]);
      expect(rowsOf(todos), [('A', TodoStatus.completed), ('C', TodoStatus.pending)]);
    });

    test('numbering from 1 again starts a fresh list, and a TodoWrite replaces it', () {
      expect(rowsOf(todosAcross([[create(1, 'A'), create(2, 'B', 1)], [create(1, 'New')]])), [('New', TodoStatus.pending)]);
      expect(rowsOf(todosAcross([[create(1, 'A')], [act('t', todoWriteTool, todos([('W', 'completed')]))]])), [('W', TodoStatus.completed)]);
      expect(todosAcross([[act('b', 'Bash')]]), isNull);
    });

    test('a failed call changes nothing', () {
      final failed = ActivityView(callId: 'x', input: {'taskId': '1', 'status': 'completed'}, offset: 1, ok: false, tool: taskUpdateTool);
      expect(rowsOf(todosAcross([[create(1, 'A'), failed]])), [('A', TodoStatus.pending)]);
    });

    test('todosByAnswer carries the list per author and only marks answers that changed it', () {
      final messages = [
        messageOf('a1', MessageRole.assistant, '', author: 'Flux', activity: [create(1, 'A'), create(2, 'B', 1)]),
        messageOf('u1', MessageRole.user, 'Do B as well'),
        messageOf('a2', MessageRole.assistant, 'Sure', author: 'Flux', activity: [act('r', 'Read')]),
        messageOf('g1', MessageRole.assistant, '', author: 'Forge', activity: [create(1, 'Other')]),
        messageOf('a3', MessageRole.assistant, '', author: 'Flux', activity: [update(1, 'completed'), create(3, 'C', 1)]),
      ];
      final by = todosByAnswer(messages);
      expect(by.keys, ['a1', 'g1', 'a3']);
      expect(rowsOf(by['a3']), [('A', TodoStatus.completed), ('B', TodoStatus.pending), ('C', TodoStatus.pending)]);
      expect(rowsOf(by['g1']), [('Other', TodoStatus.pending)]);
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

    testWidgets('AnswerBody draws the list carried over from earlier answers', (tester) async {
      final m = messageOf('m', MessageRole.assistant, '', activity: [update(1, 'completed'), update(2, 'in_progress', 1)]);
      final carried = todosAcross([
        [create(1, 'Old step'), create(2, 'Next step', 1)],
        m.activity,
      ]);
      await tester.pumpWidget(MaterialApp(home: Scaffold(body: AnswerBody(message: m, todos: carried))));
      expect(find.byKey(const Key('todo-card')), findsOneWidget);
      expect(find.text('1 of 2 done'), findsOneWidget);
      expect(find.text('Next step…'), findsOneWidget);
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
