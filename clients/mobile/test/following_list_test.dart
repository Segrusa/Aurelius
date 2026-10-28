import 'package:aurelius_mobile/features/chat/following_list.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// A conversation of [count] rows of 100 px; the last one [lastHeight] high
/// (the answer that is streaming).
List<FollowItem> rows(int count, {double lastHeight = 100}) => [
      for (var i = 0; i < count; i++)
        FollowItem('m$i', SizedBox(height: i == count - 1 ? lastHeight : 100, child: Text('row $i'))),
    ];

Widget app(List<FollowItem> items, {int? liveFrom}) => MaterialApp(
      home: Scaffold(body: SizedBox(height: 600, child: FollowingList(key: const Key('list'), items: items, liveFrom: liveFrom))),
    );

ScrollPosition position(WidgetTester tester) => tester.state<ScrollableState>(find.byType(Scrollable)).position;

void main() {
  testWidgets('at the end, new text is followed', (tester) async {
    await tester.pumpWidget(app(rows(20, lastHeight: 100), liveFrom: 19));
    await tester.pumpAndSettle();
    expect(position(tester).pixels, position(tester).maxScrollExtent);

    // The answer grows and a new message comes in: still at the end.
    await tester.pumpWidget(app([...rows(20, lastHeight: 400), FollowItem('m20', const SizedBox(height: 100, child: Text('row 20')))], liveFrom: 19));
    await tester.pump();
    expect(position(tester).pixels, position(tester).maxScrollExtent);
    expect(find.text('row 20'), findsOneWidget);
    expect(find.byKey(const Key('scroll-to-end')), findsNothing);
  });

  testWidgets('scrolled up, the view holds still while text comes in', (tester) async {
    await tester.pumpWidget(app(rows(20, lastHeight: 100), liveFrom: 19));
    await tester.pumpAndSettle();

    await tester.timedDrag(find.byType(Scrollable), const Offset(0, 500), const Duration(seconds: 1));
    await tester.pumpAndSettle();
    final reading = find.text('row 12');
    expect(reading, findsOneWidget);
    final before = tester.getTopLeft(reading);
    final pixels = position(tester).pixels;
    expect(find.byKey(const Key('scroll-to-end')), findsOneWidget);
    expect(find.byKey(const Key('scroll-to-end-fresh')), findsNothing);

    // The streaming answer grows, then two new messages arrive.
    await tester.pumpWidget(app(rows(20, lastHeight: 900), liveFrom: 19));
    await tester.pump();
    await tester.pumpWidget(app([
      ...rows(20, lastHeight: 900),
      FollowItem('m20', const SizedBox(height: 100, child: Text('row 20'))),
      FollowItem('m21', const SizedBox(height: 100, child: Text('row 21'))),
    ], liveFrom: 19));
    await tester.pumpAndSettle();

    expect(tester.getTopLeft(reading), before);
    expect(position(tester).pixels, pixels);
    expect(find.byKey(const Key('scroll-to-end-fresh')), findsOneWidget);

    // The button goes back down and follows again.
    await tester.tap(find.byKey(const Key('scroll-to-end')));
    await tester.pumpAndSettle();
    expect(position(tester).pixels, position(tester).maxScrollExtent);
    expect(find.text('row 21'), findsOneWidget);
    expect(find.byKey(const Key('scroll-to-end')), findsNothing);

    await tester.pumpWidget(app([
      ...rows(20, lastHeight: 900),
      FollowItem('m20', const SizedBox(height: 100, child: Text('row 20'))),
      FollowItem('m21', const SizedBox(height: 300, child: Text('row 21'))),
    ], liveFrom: 19));
    await tester.pump();
    expect(position(tester).pixels, position(tester).maxScrollExtent);
  });

  testWidgets('a short conversation does not scroll into empty space after replies', (tester) async {
    // Two rows when the list opens, then two replies below the anchor: all
    // 400 px fit in the 600 px view, so there is nothing to scroll.
    await tester.pumpWidget(app(rows(2)));
    await tester.pumpAndSettle();
    await tester.pumpWidget(app(rows(4)));
    await tester.pumpAndSettle();
    expect(position(tester).minScrollExtent, position(tester).maxScrollExtent);

    await tester.timedDrag(find.byType(Scrollable), const Offset(0, 400), const Duration(seconds: 1));
    await tester.pumpAndSettle();
    expect(position(tester).pixels, position(tester).maxScrollExtent);
    expect(find.text('row 3'), findsOneWidget);
    expect(find.byKey(const Key('scroll-to-end')), findsNothing);

    // Grown past the screen: scrolling up stops at the first row.
    await tester.pumpWidget(app(rows(9)));
    await tester.pumpAndSettle();
    await tester.timedDrag(find.byType(Scrollable), const Offset(0, 2000), const Duration(seconds: 1));
    await tester.pumpAndSettle();
    expect(tester.getTopLeft(find.text('row 0')).dy, closeTo(tester.getTopLeft(find.byType(Scrollable)).dy + 12, 0.5));
  });

  testWidgets('a little above the end still counts as the end', (tester) async {
    await tester.pumpWidget(app(rows(20), liveFrom: 19));
    await tester.pumpAndSettle();
    await tester.timedDrag(find.byType(Scrollable), const Offset(0, 30), const Duration(seconds: 1));
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('scroll-to-end')), findsNothing);
    expect(tester.state<FollowingListState>(find.byKey(const Key('list'))).following, isTrue);
  });
}
