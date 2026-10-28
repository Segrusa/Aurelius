import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/widgets/ios.dart';
import 'package:aurelius_mobile/core/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Widget host(Widget child) => MaterialApp(theme: aureliusTheme(), home: Scaffold(body: Center(child: child)));

  testWidgets('InfoIcon is a labelled 44px button; a tap shows the text', (tester) async {
    final handle = tester.ensureSemantics();
    await tester.pumpWidget(host(const InfoIcon('Changes apply from the next run.', label: 'MCP servers')));

    expect(find.bySemanticsLabel('Info: MCP servers'), findsOneWidget);
    expect(tester.getSize(find.byType(InfoIcon)), const Size(44, 44));
    expect(find.text('Changes apply from the next run.'), findsNothing);

    await tester.tap(find.byType(InfoIcon));
    await tester.pumpAndSettle();
    expect(find.text('Changes apply from the next run.'), findsOneWidget);
    handle.dispose();
  });

  testWidgets('GroupHeader shows the icon only with info', (tester) async {
    await tester.pumpWidget(host(const Column(children: [GroupHeader('Plain'), GroupHeader('Explained', info: 'Why.')])));
    expect(find.byType(InfoIcon), findsOneWidget);
  });
}
