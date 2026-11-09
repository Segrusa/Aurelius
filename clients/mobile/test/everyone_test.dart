import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:aurelius_mobile/data/channels.dart';
import 'package:aurelius_mobile/data/everyone.dart';
import 'package:aurelius_mobile/data/protocol/models.dart';
import 'package:aurelius_mobile/features/chat/everyone_audience.dart';
import 'package:aurelius_mobile/features/team/mentions.dart';

DiscussionAudience audience({int employees = 5, int paused = 1, int boardMembers = 2}) => DiscussionAudience(
      boardMembers: boardMembers,
      boardUsernames: const [],
      employeeNames: const [],
      employees: employees,
      paused: paused,
      pausedNames: const [],
    );

void main() {
  test('the audience line counts employees, paused ones and board members', () {
    expect(everyoneAudienceLine(audience()), 'Wakes 5 employees (1 paused) and notifies 2 board members');
    expect(everyoneAudienceLine(audience(employees: 1, paused: 0, boardMembers: 1)), 'Wakes 1 employee and notifies 1 board member');
    expect(everyoneAudienceLine(audience(boardMembers: 0)), 'Wakes 5 employees (1 paused)');
  });

  test('@Everyone is found as a whole word in any case, not inside an email or a longer word', () {
    for (final yes in ['@Everyone', 'hi @EVERYONE!', '(@everyone)', 'a\n@everyone, b', '@everyone.']) {
      expect(mentionsEveryone(yes), isTrue, reason: yes);
    }
    for (final no in ['me@everyone.com', '@everyone2', '@everyones', 'everyone', '@every']) {
      expect(mentionsEveryone(no), isFalse, reason: no);
    }
  });

  test('Everyone is suggested first only when the server has the feature', () {
    expect(mentionSuggestions('hi @e', withEveryone(['Eva'], enabled: true))!.$2, ['Everyone', 'Eva']);
    expect(mentionSuggestions('hi @e', withEveryone(['Eva'], enabled: false))!.$2, ['Eva']);
    expect(mentionSuggestions('@Everyone', withEveryone(['Eva'], enabled: true)), isNull);
  });

  test('@Everyone is marked as a mention in any case when it is known', () {
    final segments = mentionSegments('ping @everyone now', known: withEveryone(['Eva'], enabled: true));
    expect(segments.map((s) => s.$1), ['ping ', '@everyone', ' now']);
    expect(segments[1].$2, MentionMark.other);
    expect(mentionSegments('ping @everyone now', known: ['Eva']).length, 1);
  });

  testWidgets('the audience is asked once per mention, not per keystroke, and a failure shows nothing', (tester) async {
    final text = ValueNotifier(TextEditingValue.empty);
    final asked = <String?>[];
    var fail = false;
    await tester.pumpWidget(MaterialApp(
      home: Scaffold(
        body: EveryoneAudienceLine(
          text: text,
          channel: 'c1',
          load: (c) async {
            asked.add(c);
            if (fail) throw StateError('old server');
            return audience();
          },
        ),
      ),
    ));
    text.value = const TextEditingValue(text: 'hi @Everyone');
    text.value = const TextEditingValue(text: 'hi @Everyone a');
    text.value = const TextEditingValue(text: 'hi @Everyone ab');
    await tester.pump(const Duration(milliseconds: everyoneAudienceDelayMs + 10));
    await tester.pump();
    expect(asked, ['c1']);
    expect(find.text('Wakes 5 employees (1 paused) and notifies 2 board members'), findsOneWidget);

    text.value = const TextEditingValue(text: 'hi');
    await tester.pump();
    expect(find.byKey(const Key('everyone-audience')), findsNothing);

    fail = true;
    text.value = const TextEditingValue(text: '@everyone');
    await tester.pump(const Duration(milliseconds: everyoneAudienceDelayMs + 10));
    await tester.pump();
    expect(asked.length, 2);
    expect(find.byKey(const Key('everyone-audience')), findsNothing);
  });
}
