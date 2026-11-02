import 'package:aurelius_mobile/features/chat/following_list.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

// Found against a real daemon (AUR-FA-M2): a link to a message far back in a long
// history of tall rows ran out of steps, and nothing was marked or said.

void main() {
  testWidgets('a row far back in a long history of rows is reached and marked', (tester) async {
    final list = GlobalKey<FollowingListState>();
    final items = [for (var i = 0; i < 1000; i++) FollowItem('m$i', SizedBox(height: 90, child: Text('Row $i')))];
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: SizedBox(height: 480, child: FollowingList(key: list, items: items)))));
    await tester.pump();
    var reached = false;
    list.currentState!.reveal('m3').then((r) => reached = r);
    for (var i = 0; i < 3000 && !reached; i++) {
      await tester.pump(const Duration(milliseconds: 16));
    }
    expect(reached, isTrue);
    // Let it scroll there.
    for (var i = 0; i < 3; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(find.byKey(const ValueKey('flash-m3')), findsOneWidget);
    await tester.pump(revealFlash);
  });

  testWidgets('a row that is not in the list is reported as not reached', (tester) async {
    final list = GlobalKey<FollowingListState>();
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: FollowingList(key: list, items: const [FollowItem('m0', Text('Row'))]))));
    expect(await list.currentState!.reveal('gone'), isFalse);
  });
}
