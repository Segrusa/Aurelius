import 'package:aurelius_mobile/core/widgets/keyboard.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Future<FocusNode> pump(WidgetTester tester) async {
    final focus = FocusNode();
    addTearDown(focus.dispose);
    await tester.pumpWidget(MaterialApp(
      builder: (context, child) => DismissKeyboardOnTap(child: child!),
      home: Scaffold(
        body: ListView(children: [
          TextField(key: const Key('field'), focusNode: focus),
          TextFieldTapRegion(child: TextButton(key: const Key('send'), onPressed: () {}, child: const Text('Send'))),
          for (var i = 0; i < 40; i++) SizedBox(height: 40, child: Text('row $i')),
        ]),
      ),
    ));
    await tester.tap(find.byKey(const Key('field')));
    await tester.pump();
    expect(focus.hasFocus, isTrue);
    return focus;
  }

  testWidgets('a tap outside the field closes the keyboard', (tester) async {
    final focus = await pump(tester);
    await tester.tap(find.text('row 5'));
    await tester.pump();
    expect(focus.hasFocus, isFalse);
  });

  testWidgets('a scroll outside the field keeps the keyboard', (tester) async {
    final focus = await pump(tester);
    await tester.drag(find.text('row 5'), const Offset(0, -200));
    await tester.pump();
    expect(focus.hasFocus, isTrue);
  });

  testWidgets('a tap on a button that belongs to the field keeps the keyboard', (tester) async {
    final focus = await pump(tester);
    await tester.tap(find.byKey(const Key('send')));
    await tester.pump();
    expect(focus.hasFocus, isTrue);
  });

  test('a touch that moves less than the slop is a tap', () {
    expect(isTap(Offset.zero, const Offset(3, 4)), isTrue);
    expect(isTap(Offset.zero, const Offset(0, 40)), isFalse);
  });
}
