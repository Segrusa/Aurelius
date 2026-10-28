import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/mode_memory.dart';
import 'package:aurelius_mobile/features/project/new_task_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

void main() {
  late MockRepository repo;

  setUp(() {
    SharedPreferences.setMockInitialValues({});
    repo = MockRepository();
    when(() => repo.host).thenReturn(const Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
  });

  test('the mode is kept per PC and employee; null forgets it', () async {
    expect(await ModeMemory.read('h', 'ada'), isNull);
    await ModeMemory.write('h', 'ada', 'auto');
    expect(await ModeMemory.read('h', 'ada'), 'auto');
    expect(await ModeMemory.read('h', 'tom'), isNull);
    expect(await ModeMemory.read('other', 'ada'), isNull);
    await ModeMemory.write('h', 'ada', null);
    expect(await ModeMemory.read('h', 'ada'), isNull);
  });

  Future<NewTask?> giveWork(WidgetTester tester, Future<void> Function() act) async {
    NewTask? result;
    final employees = [employeeOf(id: 'ada', name: 'Ada'), employeeOf(id: 'tom', name: 'Tom')];
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: MaterialApp(
        theme: aureliusTheme(),
        home: Builder(
          builder: (context) => Scaffold(body: TextButton(onPressed: () async => result = await NewTaskPage.open(context, employees, employeeId: 'ada'), child: const Text('open'))),
        ),
      ),
    ));
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    await act();
    return result;
  }

  testWidgets('the last mode of the lead is the default and shows as a chip', (tester) async {
    await ModeMemory.write('h', 'ada', 'auto');
    await giveWork(tester, () async {
      expect(find.text('Mode: Auto'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('lead-tom')));
      await tester.pumpAndSettle();
      expect(find.text('Mode: Employee default'), findsOneWidget, reason: 'Tom has no remembered mode');
      await tester.tap(find.byKey(const ValueKey('lead-ada')));
      await tester.pumpAndSettle();
      expect(find.text('Mode: Auto'), findsOneWidget);
    });
  });

  testWidgets('the chip cycles through the modes and creating the task remembers the choice', (tester) async {
    final result = await giveWork(tester, () async {
      expect(find.text('Mode: Employee default'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('new-task-mode-chip')));
      await tester.pump();
      expect(find.textContaining('Mode: Ask'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('new-task-mode-chip')));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('new-task-mode-chip')));
      await tester.pump();
      expect(find.text('Mode: Auto'), findsOneWidget);
      await tester.enterText(find.widgetWithText(TextField, 'Title'), 'Ship it');
      await tester.pump();
      await tester.tap(find.widgetWithText(FilledButton, 'Create'));
      await tester.pumpAndSettle();
    });
    expect(result!.run['mode'], 'auto');
    expect(await ModeMemory.read('h', 'ada'), 'auto');
  });

  testWidgets('a mode picked here beats the remembered one', (tester) async {
    await ModeMemory.write('h', 'ada', 'plan');
    await giveWork(tester, () async {
      expect(find.text('Mode: Plan'), findsOneWidget);
      // Cycling past Plan lands on the employee default.
      await tester.tap(find.byKey(const ValueKey('new-task-mode-chip')));
      await tester.pump();
      expect(find.text('Mode: Employee default'), findsOneWidget);
    });
  });
}
