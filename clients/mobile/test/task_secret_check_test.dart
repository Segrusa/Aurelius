import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/vault.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/dashboard/give_work.dart';
import 'package:aurelius_mobile/features/project/new_task_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'support/builders.dart';

// QA2 m8 (B10): a new task whose title holds a vault value. The form sends it
// itself, keeps what was typed and answers with a real dialog, not a toast
// that asks without a button. Since AUR-MU-04-F5 `task.create` takes
// `confirm_secret`: Send anyway creates it, refused again it is a notice (M4).
// Neutral names and a fake secret only.

class MockRepository extends Mock implements AureliusRepository {}

const fakeSecret = 'sk-test-FAKE-0000';
const refused = ProtocolException(ErrorCode.looksLikeSecret, 'This looks like a vault secret: GitHub token. Send anyway?', matchedItems: ['GitHub token']);

void main() {
  late MockRepository repo;

  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  setUp(() {
    SharedPreferences.setMockInitialValues({});
    repo = MockRepository();
    when(() => repo.host).thenReturn(const Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
  });

  void writes(Future<Receipt> Function() answer) =>
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenAnswer((_) => answer());

  int writeCount() => verify(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).callCount;

  /// Opens the new-task form as the project's + does: it sends through [writeTask].
  Future<List<NewTask?>> openForm(WidgetTester tester) async {
    final results = <NewTask?>[];
    final employees = [employeeOf(id: 'ada', name: 'Ada')];
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: MaterialApp(
        theme: aureliusTheme(),
        home: Builder(
          builder: (context) => Scaffold(
            body: TextButton(
              onPressed: () async => results.add(await NewTaskPage.open(context, employees, submit: (form, task) async => await writeTask(form, 'p1', task) != null)),
              child: const Text('open'),
            ),
          ),
        ),
      ),
    ));
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    return results;
  }

  Future<void> create(WidgetTester tester) async {
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pumpAndSettle();
  }

  testWidgets('refused as a vault secret: the form stays with what was typed, a dialog names the item and Cancel goes back to it', (tester) async {
    writes(() async => throw refused);
    final results = await openForm(tester);
    await tester.enterText(find.widgetWithText(TextField, 'Title'), 'Deploy with $fakeSecret');
    await tester.enterText(find.widgetWithText(TextField, 'Goal (optional)'), 'Release 2.1');
    await tester.pump();
    await create(tester);

    expect(find.byKey(const ValueKey('secret-check')), findsOneWidget, reason: 'a dialog with a button, not a toast');
    expect(find.text(secretCheckTitle), findsOneWidget);
    expect(find.text(secretCheckBody(['GitHub token'])), findsOneWidget);
    expect(find.descendant(of: find.byKey(const ValueKey('secret-check')), matching: find.textContaining(fakeSecret)), findsNothing);
    expect(find.byType(SnackBar), findsNothing);
    expect(find.byKey(const ValueKey('secret-check-send')), findsOneWidget, reason: 'task.create takes confirm_secret (AUR-MU-04-F5)');

    await tester.tap(find.byKey(const ValueKey('secret-check-cancel')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('secret-check')), findsNothing);
    expect(find.text('New task'), findsOneWidget, reason: 'the form is still open');
    expect(find.text('Deploy with $fakeSecret'), findsOneWidget);
    expect(find.text('Release 2.1'), findsOneWidget);
    expect(results, isEmpty, reason: 'the form did not return');
    expect(writeCount(), 1, reason: 'sent once, nothing again on its own');
  });

  testWidgets('after taking the value out it is created, and the form closes with the task', (tester) async {
    var calls = 0;
    writes(() async => calls++ == 0 ? throw refused : receiptOf(const CommandResultTaskCreated(taskId: 't1')));
    final results = await openForm(tester);
    await tester.enterText(find.widgetWithText(TextField, 'Title'), 'Deploy with $fakeSecret');
    await tester.pump();
    await create(tester);
    await tester.tap(find.byKey(const ValueKey('secret-check-cancel')));
    await tester.pumpAndSettle();
    await tester.enterText(find.widgetWithText(TextField, 'Title'), 'Deploy with GITHUB_TOKEN from the vault');
    await tester.pump();
    await create(tester);
    expect(find.text('New task'), findsNothing);
    expect(results.single?.title, 'Deploy with GITHUB_TOKEN from the vault');
    final sent = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).captured;
    expect(sent.map((c) => (c as Map<String, dynamic>)['method']), ['task.create', 'task.create']);
  });

  List<Map<String, dynamic>> sent() => [
        for (final c in verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).captured) c as Map<String, dynamic>,
      ];

  testWidgets('Send anyway creates it with confirm_secret, and the form closes with the task', (tester) async {
    var calls = 0;
    writes(() async => calls++ == 0 ? throw refused : receiptOf(const CommandResultTaskCreated(taskId: 't1')));
    final results = await openForm(tester);
    await tester.enterText(find.widgetWithText(TextField, 'Title'), 'Deploy with $fakeSecret');
    await tester.pump();
    await create(tester);
    await tester.tap(find.byKey(const ValueKey('secret-check-send')));
    await tester.pumpAndSettle();
    expect(find.text('New task'), findsNothing);
    expect(results.single?.title, 'Deploy with $fakeSecret');
    final requests = sent();
    expect(requests.map((w) => w['method']), ['task.create', 'task.create']);
    expect((requests[0]['params'] as Map).containsKey('confirm_secret'), isFalse);
    expect((requests[1]['params'] as Map)['confirm_secret'], isTrue);
  });

  testWidgets('refused again after Send anyway: a notice, no second dialog, and the form keeps the input (no loop)', (tester) async {
    writes(() async => throw refused);
    final results = await openForm(tester);
    await tester.enterText(find.widgetWithText(TextField, 'Title'), 'Deploy with $fakeSecret');
    await tester.pump();
    await create(tester);
    await tester.tap(find.byKey(const ValueKey('secret-check-send')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('secret-check')), findsNothing, reason: 'no second question');
    expect(find.text(secretCheckNoSendAnyway), findsOneWidget);
    expect(find.text('Deploy with $fakeSecret'), findsOneWidget);
    expect(results, isEmpty);
    final requests = sent();
    expect(requests, hasLength(2), reason: 'one refused send and one refused Send anyway, nothing more');
    expect((requests[1]['params'] as Map)['confirm_secret'], isTrue);
  });

  testWidgets('another refusal is a notice; the form keeps the input too', (tester) async {
    writes(() async => throw const ProtocolException(ErrorCode.validation, 'The title is too long.'));
    final results = await openForm(tester);
    await tester.enterText(find.widgetWithText(TextField, 'Title'), 'Ship it');
    await tester.pump();
    await create(tester);
    expect(find.text('The title is too long.'), findsOneWidget);
    expect(find.byKey(const ValueKey('secret-check')), findsNothing);
    expect(find.text('Ship it'), findsOneWidget);
    expect(results, isEmpty);
  });
}
