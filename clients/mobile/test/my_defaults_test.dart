import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/protocol/models.dart';
import 'package:aurelius_mobile/features/my_defaults/my_defaults_bloc.dart';
import 'package:aurelius_mobile/features/my_defaults/my_defaults_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

const _connections = ConnectionsSnapshot(asOfSeq: 1, defaults: [], connections: [
  ConnectionView(authKind: 'oauth', billing: 'subscription', displayName: 'Claude', hasSecret: false, id: 'c1', models: ['claude-opus-5-5', 'claude-sonnet-5-5'], provider: 'claude_code', status: 'ready'),
]);

UserRunDefaultsView _view(UserRunDefaults defaults) =>
    UserRunDefaultsView(defaults: defaults, model: defaults.model ?? 'claude-sonnet-5-5', supportedEfforts: const ['low', 'medium', 'high'], modelDefaultEffort: 'medium');

/// A repository that keeps the defaults it is given, as the server does.
MockRepository _repository({UserRunDefaults start = const UserRunDefaults()}) {
  final repo = MockRepository();
  var stored = start;
  when(() => repo.myRunDefaults()).thenAnswer((_) async => _view(stored));
  when(() => repo.connections()).thenAnswer((_) async => _connections);
  when(() => repo.setMyRunDefaults(any())).thenAnswer((call) async {
    stored = call.positionalArguments.first as UserRunDefaults;
    return receiptOf(CommandResultUserRunDefaultsSet(changed: true, defaults: stored));
  });
  return repo;
}

Future<void> _open(WidgetTester tester, MockRepository repo) async {
  await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(value: repo, child: const MaterialApp(home: MyDefaultsPage())));
  await tester.pumpAndSettle();
}

void main() {
  setUpAll(() => registerFallbackValue(const UserRunDefaults()));

  group('My defaults', () {
    test('one field changes, the others stay and null clears it', () {
      const start = UserRunDefaults(model: 'claude-opus-5-5', mode: AgentMode.plan);
      final effort = withMyDefault(start, MyDefaultsField.effort, 'high');
      expect((effort.model, effort.effort, effort.mode), ('claude-opus-5-5', 'high', AgentMode.plan));
      final cleared = withMyDefault(effort, MyDefaultsField.model, null);
      expect((cleared.model, cleared.effort), (null, 'high'));
      expect(withMyDefault(start, MyDefaultsField.mode, 'auto').mode, AgentMode.auto);
    });

    test('Default names what it comes to', () {
      expect(myModelLabel('claude-opus-5-5'), 'Opus 5.5');
      expect(myModelLabel(null, fallback: 'claude-sonnet-5-5'), 'Default · Sonnet 5.5');
      expect(myModelLabel(null), 'Default');
    });

    testWidgets('shows what is chosen and what Default comes to', (tester) async {
      await _open(tester, _repository(start: const UserRunDefaults(mode: AgentMode.plan)));
      expect(find.text('Default · Sonnet 5.5'), findsOneWidget);
      expect(find.text('Plan'), findsOneWidget);
      expect(find.text('Default · Sonnet'), findsOneWidget, reason: 'the subagent model');
    });

    testWidgets('a chosen model is sent as the whole defaults and shown', (tester) async {
      final repo = _repository(start: const UserRunDefaults(mode: AgentMode.plan));
      await _open(tester, repo);
      await tester.tap(find.byKey(const ValueKey('my-default-model')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('my-default-option-claude-opus-5-5')));
      await tester.pumpAndSettle();
      final sent = verify(() => repo.setMyRunDefaults(captureAny())).captured.single as UserRunDefaults;
      expect((sent.model, sent.mode), ('claude-opus-5-5', AgentMode.plan), reason: 'the mode stays');
      expect(find.text('Opus 5.5'), findsOneWidget);
    });

    testWidgets('a failed read shows what failed and Try again, never an endless loading', (tester) async {
      final repo = _repository();
      when(() => repo.myRunDefaults()).thenThrow(Exception('offline'));
      await _open(tester, repo);
      expect(find.text('Could not load your defaults.'), findsOneWidget);
      expect(find.text('Try again'), findsOneWidget);
    });
  });
}
