import 'package:aurelius_mobile/app/router.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/widgets/ios.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

void main() {
  swipeBackTests();
  for (final platform in [TargetPlatform.iOS, TargetPlatform.android]) {
    testWidgets('pulling a large-title page down refreshes it (${platform.name})', (tester) async {
      debugDefaultTargetPlatformOverride = platform;
      var refreshed = 0;
      await tester.pumpWidget(MaterialApp(
        theme: aureliusTheme(),
        home: LargeTitleScaffold(
          title: 'Chat',
          onRefresh: () async => refreshed++,
          children: [for (var i = 0; i < 30; i++) ListTile(title: Text('Row $i'))],
        ),
      ));
      await tester.fling(find.text('Row 0'), const Offset(0, 400), 1000);
      await tester.pumpAndSettle();
      expect(refreshed, 1);
      debugDefaultTargetPlatformOverride = null;
    });
  }
}

/// Drag from the left edge to the right, as the iOS back gesture.
Future<void> edgeSwipe(WidgetTester tester) async {
  final g = await tester.startGesture(const Offset(5, 300));
  for (var i = 0; i < 20; i++) {
    await g.moveBy(const Offset(20, 0));
    await tester.pump(const Duration(milliseconds: 16));
  }
  await g.up();
  await tester.pumpAndSettle();
}

void swipeBackTests() {
  testWidgets('a pushed screen goes back with the iOS edge swipe', (tester) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.iOS;
    final router = GoRouter(initialLocation: '/a', routes: [
      GoRoute(path: '/a', pageBuilder: (c, s) => routePage(s, Scaffold(body: Center(child: TextButton(onPressed: () => c.push('/d'), child: const Text('open')))))),
      GoRoute(path: '/d', pageBuilder: (c, s) => routePage(s, const Scaffold(appBar: GlassAppBar(title: Text('Detail')), body: Text('detail body')))),
    ]);
    await tester.pumpWidget(MaterialApp.router(theme: aureliusTheme(), routerConfig: router));
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    expect(find.text('detail body'), findsOneWidget);
    await edgeSwipe(tester);
    expect(find.text('detail body'), findsNothing);
    debugDefaultTargetPlatformOverride = null;
  });
}
