import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/answer_body.dart';
import 'package:aurelius_mobile/features/chat/collapse.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/builders.dart';

Widget app(Widget child) => MaterialApp(theme: aureliusTheme(), home: Scaffold(body: SingleChildScrollView(child: child)));

String items(int n) => [for (var i = 1; i <= n; i++) '- item $i'].join('\n');

Future<void> tapText(WidgetTester tester, String text) async {
  await tester.ensureVisible(find.text(text));
  await tester.pump();
  await tester.tap(find.text(text));
  await tester.pump();
}

Widget answer(MessageView m) => Collapsible(id: m.id, text: m.text, streaming: m.isStreaming, builder: (context, cut) => AnswerBody(message: m, cut: cut));

void main() {
  setUp(Collapsible.expanded.clear);

  test('a message is cut after its 20th line, code blocks and trailing newlines as written', () {
    expect(collapseCut(items(20)), isNull);
    expect(collapseCut('${items(20)}\n\n\n'), isNull);
    final text = items(23);
    final (cut, hidden) = collapseCut(text)!;
    expect(text.substring(0, cut), items(20));
    expect(hidden, 3);
    expect(collapseCut('```\n${items(19)}\n```')!.$2, 1);
  });

  test('blank lines count as lines', () {
    final text = [for (var i = 1; i <= 12; i++) 'para $i'].join('\n\n');
    final (cut, hidden) = collapseCut(text)!;
    // The 20th line is the blank one after the 10th paragraph.
    expect(text.substring(0, cut), '${[for (var i = 1; i <= 10; i++) 'para $i'].join('\n\n')}\n');
    expect(hidden, 3);
  });

  test('a cut inside a code block closes it, with the same fence', () {
    final code = 'Run:\n\n````sh\n${items(30)}\n````\nDone.';
    final (cut, _) = collapseCut(code)!;
    expect(collapsedMarkdown(code, cut), 'Run:\n\n````sh\n${items(17)}\n````');

    final tilde = '  ~~~\n${items(30)}\n  ~~~';
    expect(collapsedMarkdown(tilde, collapseCut(tilde)!.$1), '  ~~~\n${items(19)}\n  ~~~');
  });

  test('a cut after a closed code block adds nothing', () {
    final text = '```dart\nx\n```\n${items(30)}';
    final cut = collapseCut(text)!.$1;
    expect(collapsedMarkdown(text, cut), text.substring(0, cut));
    // A shorter or different fence, or one with text after it, does not close.
    final open = '````\n```\n~~~~\n```` x\n${items(30)}';
    expect(collapsedMarkdown(open, collapseCut(open)!.$1), endsWith('- item 16\n````'));
    // Backticks in the info string make it no fence.
    final inline = '``` a ` b\n${items(30)}';
    expect(collapsedMarkdown(inline, collapseCut(inline)!.$1), inline.substring(0, collapseCut(inline)!.$1));
  });

  testWidgets('a folded answer cut inside a code block shows it as code', (tester) async {
    final m = messageOf('a1', MessageRole.assistant, 'Run:\n```\n${items(30)}\n```');
    await tester.pumpWidget(app(answer(m)));
    expect(find.textContaining('- item 18'), findsOneWidget);
    expect(find.textContaining('- item 19'), findsNothing);
    expect(find.textContaining('```'), findsNothing);
    expect(find.text('Show more (13 lines)'), findsOneWidget);
  });

  testWidgets('a long answer shows 20 lines and unfolds and folds again', (tester) async {
    final text = items(25);
    final m = messageOf('a1', MessageRole.assistant, text, activity: [
      ActivityView(callId: 't1', tool: 'Bash', input: const {'description': 'Late step'}, ok: true, offset: text.indexOf('- item 23')),
    ]);
    await tester.pumpWidget(app(answer(m)));
    expect(find.text('item 20'), findsOneWidget);
    expect(find.text('item 21'), findsNothing);
    expect(find.text('Late step'), findsNothing);
    expect(find.text('Show more (5 lines)'), findsOneWidget);

    await tapText(tester, 'Show more (5 lines)');
    expect(find.text('item 25', skipOffstage: false), findsOneWidget);
    expect(find.text('Late step', skipOffstage: false), findsOneWidget);
    await tapText(tester, 'Show less');
    expect(find.text('item 21'), findsNothing);
  });

  testWidgets('the choice outlives the widget, as when the list scrolls it away', (tester) async {
    final m = messageOf('a1', MessageRole.assistant, items(25));
    await tester.pumpWidget(app(answer(m)));
    await tapText(tester, 'Show more (5 lines)');
    await tester.pumpWidget(app(const SizedBox()));
    await tester.pumpWidget(app(answer(m)));
    expect(find.text('item 25', skipOffstage: false), findsOneWidget);
    expect(find.text('Show less', skipOffstage: false), findsOneWidget);
  });

  testWidgets('a streaming answer is never cut and stays unfolded when it ends', (tester) async {
    await tester.pumpWidget(app(answer(messageOf('a1', MessageRole.assistant, items(25), status: MessageStatus.partial))));
    expect(find.text('item 25', skipOffstage: false), findsOneWidget);
    expect(find.byKey(const Key('collapse-toggle')), findsNothing);

    await tester.pumpWidget(app(answer(messageOf('a1', MessageRole.assistant, items(30)))));
    expect(find.text('item 30', skipOffstage: false), findsOneWidget);
    expect(find.text('Show less', skipOffstage: false), findsOneWidget);
  });
}
