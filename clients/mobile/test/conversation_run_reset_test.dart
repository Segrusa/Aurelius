import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/run_config.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/chat_bloc.dart';
import 'package:aurelius_mobile/features/chat/chat_view.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

class MockRuntimeBloc extends MockBloc<RuntimeEvent, RuntimeState> implements RuntimeBloc {}

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

const employeeId = '01900000-0000-7000-8000-000000000005';
const connectionId = '01900000-0000-7000-8000-00000000003c';

Map<String, dynamic> setting(String? value) => value == null ? {'mode': 'inherit'} : {'mode': 'value', 'value': value};

/// One Claude Code connection; the employee's default is Opus · High.
RunConfig config() => RunConfig.of(ConnectionsSnapshot.fromJson({
      'as_of_seq': 1,
      'connections': [
        {
          'id': connectionId,
          'provider': 'claude_code',
          'display_name': 'Claude Code',
          'auth_kind': 'subscription',
          'billing': 'subscription',
          'status': 'ready',
          'has_secret': false,
          'default_model': 'sonnet',
          'models': ['opus', 'sonnet'],
        },
      ],
      'defaults': [
        {
          'scope': {'kind': 'global'},
          'purpose': 'employee',
          'settings': {'connection': setting(connectionId), 'model': setting(null), 'persona': setting(null), 'effort': setting(null)},
        },
        {
          'scope': {'kind': 'employee', 'id': employeeId},
          'purpose': 'employee',
          'settings': {'connection': setting(null), 'persona': setting(null), 'model': setting('opus'), 'effort': setting('high')},
        },
      ],
    }));

/// "Back to employee default" (chip undo, model sheet): without a task choice
/// it clears this conversation's override so it follows the default again;
/// over a task's own choice it pins the resolved default, as the desktop.
void main() {
  group('ConversationRun reset', () {
    ConversationRun run({TaskView? task, String? model, String? effort}) =>
        ConversationRun(fallback: config().employeeRun(employeeId), task: task, model: model, effort: effort);

    test('an employee chat clears its override', () {
      final r = run(model: 'sonnet', effort: 'low');
      expect(r.overridden, isTrue);
      expect((r.modelReset, r.effortReset), (null, null));
    });

    test('a task conversation without a task choice clears its override', () {
      final r = run(task: taskOf(conversationId: 'c1'), model: 'sonnet');
      expect((r.modelReset, r.effortReset), (null, null));
    });

    test('over a task\'s own model or effort it pins the resolved default', () {
      for (final own in const [TaskRun(model: 'sonnet'), TaskRun(effort: 'low')]) {
        final r = run(task: taskOf(conversationId: 'c1', run: own));
        expect((r.modelReset, r.effortReset), ('opus', 'high'));
      }
    });
  });

  group('Override chip one-tap reset', () {
    late MockRepository repo;

    setUpAll(() {
      registerFallbackValue(<String, dynamic>{});
      registerFallbackValue(const RuntimeRefreshed());
      registerFallbackValue(const CompanyRefreshed());
    });

    Future<void> pump(WidgetTester tester, {TaskRun conversationRun = const TaskRun(), List<TaskView> tasks = const [], LinkState link = const LinkState(status: LinkStatus.online), UserRunDefaultsView? mine}) async {
      repo = MockRepository();
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.linkState).thenReturn(link);
      when(() => repo.messages('c1')).thenAnswer((_) async => snapshotOf('c1', run: conversationRun));
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultConversationRunSet(conversationId: 'c1')));
      final runtime = MockRuntimeBloc();
      whenListen(runtime, const Stream<RuntimeState>.empty(), initialState: RuntimeState(loading: false, snapshot: config(), mine: mine));
      final company = MockCompanyBloc();
      whenListen(company, const Stream<CompanyState>.empty(), initialState: CompanyState(loading: false, employees: [employeeOf(id: employeeId, name: 'Iris')], tasks: tasks));
      // Owned by the provider: closed as the tree unmounts, inside the test's
      // fake time (a close in addTearDown runs outside it and never ends).
      await tester.pumpWidget(MultiBlocProvider(
        providers: [
          BlocProvider<RuntimeBloc>.value(value: runtime),
          BlocProvider<CompanyBloc>.value(value: company),
          BlocProvider<ChatBloc>(create: (_) => ChatBloc(repo, conversationId: 'c1', employeeId: employeeId)),
        ],
        child: MaterialApp(theme: aureliusTheme(), home: const Scaffold(body: Center(child: ConversationModelChip()))),
      ));
      await tester.pump(const Duration(milliseconds: 300));
    }

    Future<Map<String, dynamic>> undo(WidgetTester tester) async {
      final chip = find.byKey(const Key('conversation-model-chip'));
      expect(tester.widget(chip), isA<InputChip>(), reason: 'an override shows the undo');
      await tester.tap(find.descendant(of: chip, matching: find.byIcon(Icons.undo)));
      await tester.pump(const Duration(milliseconds: 50));
      return writesOf(repo).single;
    }

    testWidgets('in an employee chat the undo clears the override', (tester) async {
      await pump(tester, conversationRun: const TaskRun(model: 'sonnet'));
      expect(await undo(tester), {
        'method': 'conversation.set_run',
        'params': {'conversation_id': 'c1'},
      });
      await tester.pump();
      expect(tester.widget(find.byKey(const Key('conversation-model-chip'))), isA<ActionChip>(), reason: 'follows the employee default again');
    });

    testWidgets('with My defaults the chip says Your default and its value, a choice only the value', (tester) async {
      const link = LinkState(status: LinkStatus.online, features: ['user_run_defaults']);
      const mine = UserRunDefaultsView(defaults: UserRunDefaults(effort: 'low'), model: 'sonnet');
      await pump(tester, link: link, mine: mine);
      expect(find.text('Your default · Sonnet 5.5 · Low'), findsOneWidget, reason: 'the board member\'s own default, not the employee\'s Opus');
      expect(find.textContaining("Employee"), findsNothing);
    });

    testWidgets('with My defaults a choice for this chat shows only the value and resets to Your default', (tester) async {
      const link = LinkState(status: LinkStatus.online, features: ['user_run_defaults']);
      const mine = UserRunDefaultsView(defaults: UserRunDefaults(effort: 'low'), model: 'sonnet');
      await pump(tester, conversationRun: const TaskRun(model: 'opus'), link: link, mine: mine);
      expect(find.text('Opus 5.5 · Low'), findsOneWidget);
      expect(tester.widget<InputChip>(find.byKey(const Key('conversation-model-chip'))).deleteButtonTooltipMessage, 'Use your default');
    });

    testWidgets('over a task\'s own model the undo pins the employee default', (tester) async {
      await pump(tester, tasks: [taskOf(conversationId: 'c1', employeeId: employeeId, run: const TaskRun(model: 'sonnet'))]);
      expect(await undo(tester), {
        'method': 'conversation.set_run',
        'params': {'conversation_id': 'c1', 'model': 'opus', 'effort': 'high'},
      });
    });
  });
}
