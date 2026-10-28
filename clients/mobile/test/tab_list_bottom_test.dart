import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/widgets/ios.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Pixel 9 in logical pixels, with the gesture bar inset at the bottom.
void pixel9(WidgetTester tester) {
  tester.view.devicePixelRatio = 2.625;
  tester.view.physicalSize = const Size(412 * 2.625, 915 * 2.625);
  tester.view.padding = const FakeViewPadding(top: 48 * 2.625, bottom: 24 * 2.625);
  addTearDown(tester.view.reset);
}

Widget page() => LargeTitleScaffold(
      title: 'Projects',
      children: [
        InsetGroup(children: [for (var i = 0; i < 12; i++) GroupRow(title: 'Project $i')]),
      ],
    );

Future<void> scrollToEnd(WidgetTester tester) async {
  for (var i = 0; i < 6; i++) {
    await tester.drag(find.byType(CustomScrollView), const Offset(0, -600));
    await tester.pumpAndSettle();
  }
}

void main() {
  for (final banner in [false, true]) {
    testWidgets('the last row of a tab list scrolls above the floating tab bar (banner: $banner)', (tester) async {
      pixel9(tester);
      await tester.pumpWidget(MaterialApp(
        theme: aureliusTheme(),
        home: TabScaffold(
          bannerVisible: banner,
          banners: [if (banner) const SizedBox(height: 40, child: Text('Offline'))],
          tabBar: GlassTabBar(
            current: 1,
            onTap: (_) {},
            tabs: const [GlassTab(CupertinoIcons.chat_bubble, 'Chat'), GlassTab(CupertinoIcons.folder, 'Projects')],
          ),
          child: page(),
        ),
      ));
      await scrollToEnd(tester);
      final barTop = tester.getTopLeft(find.byType(GlassTabBar)).dy;
      final lastRow = tester.getRect(find.text('Project 11'));
      expect(lastRow.bottom, lessThanOrEqualTo(barTop));
    });
  }

  testWidgets('a list off the tabs ends at the home-indicator inset plus a margin', (tester) async {
    pixel9(tester);
    late BuildContext inner;
    await tester.pumpWidget(MaterialApp(
      theme: aureliusTheme(),
      home: Scaffold(body: Builder(builder: (c) {
        inner = c;
        return page();
      })),
    ));
    expect(listBottom(inner), 24 + 24);
  });
}
