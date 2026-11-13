import 'dart:async';
import 'dart:typed_data';

import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/answer_body.dart';
import 'package:aurelius_mobile/features/chat/subagent_chip.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/inbox/question_form.dart';
import 'package:aurelius_mobile/features/company_org/employee_page.dart';
import 'package:aurelius_mobile/features/project/new_task_page.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

class MockRuntimeBloc extends MockBloc<RuntimeEvent, RuntimeState> implements RuntimeBloc {}

class MockInboxBloc extends MockBloc<InboxEvent, InboxState> implements InboxBloc {}

const online = LinkState(status: LinkStatus.online, capabilities: ['chat']);

EmployeeSummary iris({List<ConversationSummary> conversations = const [], List<AgentView> agents = const [], String? title, String? reportsTo, String? instructions}) =>
    employeeOf(id: 'mgr', name: 'Iris', role: 'Marketing', title: title, reportsTo: reportsTo, conversationId: 'own', conversations: conversations, agents: agents, instructions: instructions);

AgentView subagent(String name, String conversationId, int sinceMs, {String detail = '', String? prompt, String? doing, int steps = 0}) =>
    AgentView(kind: 'subagent', name: name, detail: detail, conversationId: conversationId, sinceMs: sinceMs, prompt: prompt, doing: doing, steps: steps);

Widget app(Widget child) => MaterialApp(theme: aureliusTheme(), home: child);

/// A 1×1 PNG.
final _png = Uint8List.fromList(const [
  0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
  0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
  0x42, 0x60, 0x82,
]);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  testWidgets('an answer shows its commands in place, with their output', (tester) async {
    final message = messageOf('a1', MessageRole.assistant, 'Let me check.Done.', activity: const [
      ActivityView(callId: 't1', tool: 'Bash', input: {'command': 'cargo test', 'description': 'Run the tests'}, output: 'line 1\nline 2', ok: true, offset: 13),
      ActivityView(callId: 't2', tool: 'Edit', input: {'file_path': r'C:\src\main.rs'}, ok: false, offset: 13),
    ]);
    await tester.pumpWidget(app(Scaffold(body: SingleChildScrollView(child: AnswerBody(message: message)))));
    expect(find.text('Let me check.'), findsOneWidget);
    // Two steps with nothing written between them share one line showing the latest.
    expect(find.text('2 steps · 1 failed'), findsOneWidget);
    expect(find.text('Edited main.rs'), findsOneWidget);
    expect(find.text('Run the tests'), findsNothing);
    await tester.tap(find.text('2 steps · 1 failed'));
    await tester.pump();
    // What it did, in words, on top; the command and output fold open on tap.
    expect(find.text('Run the tests'), findsOneWidget);
    expect(find.text('> cargo test'), findsNothing);
    await tester.tap(find.text('Run the tests'));
    await tester.pump();
    expect(find.text('> cargo test'), findsOneWidget);
    expect(find.text('line 1\nline 2'), findsOneWidget);
    expect(find.text('Edited main.rs'), findsOneWidget);
    expect(find.text('✗ failed'), findsOneWidget);
    expect(find.text('Done.'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('B1-2: a recurring task, new or set again, runs on your own Claude subscription; a one-off task does not say so', (tester) async {
    final employees = [iris()];
    await tester.pumpWidget(app(Builder(
      builder: (context) => Scaffold(
        body: Column(children: [
          TextButton(onPressed: () => NewTaskPage.open(context, employees, recurring: true), child: const Text('recurring')),
          TextButton(onPressed: () => NewTaskPage.open(context, employees), child: const Text('once')),
        ]),
      ),
    )));
    await tester.tap(find.text('recurring'));
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.byKey(const ValueKey('schedule-subscription')));
    expect(find.text('Tasks of this app run on your own Claude subscription.'), findsOneWidget);
    Navigator.of(tester.element(find.byType(NewTaskPage))).pop();
    await tester.pumpAndSettle();
    await tester.tap(find.text('once'));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('schedule-subscription')), findsNothing);
  });

  testWidgets('a new task is one screen: title, criteria per line, lead employee', (tester) async {
    NewTask? result;
    final employees = [iris(), employeeOf(id: 'mgr-2', name: 'Atlas')];
    await tester.pumpWidget(app(Builder(
      builder: (context) => Scaffold(body: TextButton(onPressed: () async => result = await NewTaskPage.open(context, employees), child: const Text('open'))),
    )));
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();

    final create = find.widgetWithText(FilledButton, 'Create');
    expect(tester.widget<FilledButton>(create).onPressed, isNull, reason: 'a title is required');
    expect(find.text('Repeat'), findsNothing, reason: 'a task has no repeat of its own; recurring tasks have their tab');
    await tester.enterText(find.widgetWithText(TextField, 'Title'), 'Launch page');
    expect(find.widgetWithText(TextField, 'Acceptance criteria (optional)'), findsNothing, reason: 'criteria wait under Advanced');
    await tester.tap(find.byKey(const ValueKey('new-task-advanced')));
    await tester.pumpAndSettle();
    await tester.enterText(find.widgetWithText(TextField, 'Acceptance criteria (optional)'), 'Loads fast\n\nWorks on mobile');
    await tester.tap(find.text('Atlas'));
    await tester.pump();
    expect(find.text('2 of at most 12'), findsOneWidget);
    await tester.tap(create);
    await tester.pumpAndSettle();

    expect(result!.title, 'Launch page');
    expect(result!.goal, isEmpty, reason: 'the PC uses the title as goal');
    expect(result!.criteria, ['Loads fast', 'Works on mobile']);
    expect(result!.employeeId, 'mgr-2');
    expect(tester.takeException(), isNull);
  });

  testWidgets("Claude's question: choose, or answer in your own words, then send", (tester) async {
    final repo = MockRepository();
    when(() => repo.host).thenReturn(const Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
    final connection = MockConnectionBloc();
    whenListen(connection, const Stream<LinkState>.empty(), initialState: online);
    final question = approvalOf(
      'q1',
      tool: ApprovalViewState.questionTool,
      effect: 'read',
      requester: 'Iris · Copy',
      conversationId: null,
      arguments: {
        'questions': [
          {
            'question': 'Which tone?',
            'header': 'Tone',
            'multiSelect': false,
            'options': [
              {'label': 'Formal', 'description': 'Like a bank'},
              {'label': 'Playful', 'description': 'Like a game'},
            ],
          },
          {
            'question': 'Which pages?',
            'header': 'Pages',
            'multiSelect': true,
            'options': [
              {'label': 'Home', 'description': ''},
              {'label': 'Pricing', 'description': ''},
            ],
          },
        ],
      },
    );
    Map<String, String>? sent;
    var skipped = false;
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: BlocProvider<ConnectionBloc>.value(
        value: connection,
        child: app(QuestionPage(approval: question, busy: false, minutes: 10, onSkip: () => skipped = true, onAnswer: (a) => sent = a)),
      ),
    ));
    expect(find.text('Iris · Copy asks you'), findsOneWidget);
    final send = find.widgetWithText(FilledButton, 'Send answer');
    expect(tester.widget<FilledButton>(send).onPressed, isNull, reason: 'every question needs an answer');
    await tester.tap(find.byKey(const Key('option-0-1')));
    await tester.pump();
    await tester.ensureVisible(find.byKey(const Key('option-1-0')));
    await tester.tap(find.byKey(const Key('option-1-0')));
    await tester.pump();
    await tester.ensureVisible(find.byKey(const Key('option-1-1')));
    await tester.tap(find.byKey(const Key('option-1-1')));
    await tester.pump();
    await tester.tap(send);
    expect(sent, {'Which tone?': 'Playful', 'Which pages?': 'Home, Pricing'});
    // Own words replace the options.
    await tester.enterText(find.byKey(const Key('own-0')), 'Warm but brief');
    await tester.pump();
    await tester.tap(send);
    expect(sent!['Which tone?'], 'Warm but brief');
    await tester.tap(find.widgetWithText(OutlinedButton, 'Skip'));
    expect(skipped, isTrue);
    expect(tester.takeException(), isNull);
  });

  group('EmployeePage', () {
    late MockRepository repo;
    late MockCompanyBloc company;

    Future<void> pump(
      WidgetTester tester,
      EmployeeSummary employee, {
      InboxState inboxState = const InboxState(),
      List<TaskView> tasks = const [],
      List<EmployeeSummary> team = const [],
      String? conversationId,
    }) async {
      company = MockCompanyBloc();
      whenListen(
        company,
        const Stream<CompanyState>.empty(),
        initialState: CompanyState(
          loading: false,
          projects: [projectOf(id: 'p', name: 'Atlas')],
          employees: [employee, ...team],
          tasks: tasks,
        ),
      );
      final connection = MockConnectionBloc();
      whenListen(connection, const Stream<LinkState>.empty(), initialState: online);
      final runtime = MockRuntimeBloc();
      whenListen(runtime, const Stream<RuntimeState>.empty(), initialState: const RuntimeState());
      final inbox = MockInboxBloc();
      whenListen(inbox, const Stream<InboxState>.empty(), initialState: inboxState);
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: MultiBlocProvider(
          providers: [
            BlocProvider<CompanyBloc>.value(value: company),
            BlocProvider<ConnectionBloc>.value(value: connection),
            BlocProvider<RuntimeBloc>.value(value: runtime),
            BlocProvider<InboxBloc>.value(value: inbox),
          ],
          // A conversation opens the conversation page (a task's, or another of the employee's), else the employee page.
          child: app(conversationId == null ? const EmployeePage(employeeId: 'mgr') : ConversationPage(conversationId: conversationId, employeeId: 'mgr')),
        ),
      ));
      await tester.pump(const Duration(milliseconds: 200));
    }

    setUp(() {
      repo = MockRepository();
      when(() => repo.host).thenReturn(const Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.changes).thenAnswer((_) => const Stream.empty());
      when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => loadOf());
      when(() => repo.powerChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.linkState).thenReturn(online);
      when(() => repo.subagents(any())).thenAnswer((_) async => []);
      when(() => repo.projects()).thenAnswer((_) async => []);
      when(() => repo.tasks(projectId: any(named: 'projectId'))).thenAnswer((_) async => []);
      when(() => repo.messages(any())).thenAnswer((i) async => snapshotOf(i.positionalArguments.first as String));
    });

    testWidgets("shows no chat of its own, the employee's tasks across projects (open ones first) and its setup", (tester) async {
      tester.view.physicalSize = const Size(800, 2400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await pump(tester, iris(title: 'Head of marketing'), tasks: [
        taskOf(id: 'done', title: 'Old task', employeeId: 'mgr', status: TaskStatus.succeeded, updatedAtMs: 9),
        taskOf(id: 'open', title: 'Open task', projectId: 'p', employeeId: 'mgr', status: TaskStatus.running, updatedAtMs: 1),
        taskOf(id: 'other', title: 'Not Iris', employeeId: 'someone'),
      ]);
      expect(find.byKey(const ValueKey('employee-page')), findsOneWidget);
      expect(find.byKey(const ValueKey('employee-chat')), findsNothing, reason: 'the group chat (Team) took its place');
      expect(find.text('Chat with Iris'), findsNothing);
      expect(find.byKey(const ValueKey('employee-give-work')), findsOneWidget);
      expect(find.text('Not Iris'), findsNothing);
      expect(tester.getTopLeft(find.text('Open task')).dy, lessThan(tester.getTopLeft(find.text('Old task')).dy));
      expect(find.text('Head of marketing · Marketing'), findsOneWidget, reason: 'title and role in the header');
      expect(find.text('Working in'), findsOneWidget);
      expect(find.text('Atlas'), findsOneWidget, reason: 'the project of its open task');
      for (final key in ['employee-model', 'employee-advanced', 'employee-edit', 'employee-boss']) {
        expect(find.byKey(ValueKey(key)), findsOneWidget, reason: key);
      }
      expect(find.byIcon(Icons.call_outlined), findsNothing, reason: 'the Call button is gone');
      verifyNever(() => repo.messages(any()));
      expect(tester.takeException(), isNull);
    });

    group('its persona, rules and guidelines (ticket #52)', () {
      const withGuidelines = LinkState(status: LinkStatus.online, capabilities: ['chat'], features: [guidelinesFeature]);
      const rules = 'You are Iris, head of marketing.\n\nRULE 01 — ROLE BOUNDARIES\nStay in your role.\n\nRULE 10 — AUTONOMY\nWork on your own.';
      const coding = EffectiveGuideline(chars: 21, id: 'g1', level: GuidelineLevel.employee, text: 'Format with rustfmt.\n', title: 'Rust style', version: 2);

      Future<void> open(WidgetTester tester) async {
        tester.view.physicalSize = const Size(800, 2400);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        await pump(tester, iris(instructions: rules));
      }

      setUp(() {
        when(() => repo.guidelineChanges).thenAnswer((_) => const Stream.empty());
      });

      testWidgets('the instructions open in full, with the rules past the third line', (tester) async {
        await open(tester);
        expect(find.text('RULE 10 — AUTONOMY'), findsNothing);
        await tester.tap(find.byKey(const ValueKey('employee-instructions')));
        await tester.pumpAndSettle();
        expect(find.text('Instructions of Iris'), findsOneWidget);
        expect(find.descendant(of: find.byKey(const ValueKey('employee-text-sheet')), matching: find.byType(SelectableText)), findsOneWidget);
        final shown = tester.widget<SelectableText>(find.byType(SelectableText));
        expect(shown.data, rules);
        expect(tester.takeException(), isNull);
      });

      testWidgets('its linked guidelines are listed and open with their text', (tester) async {
        when(() => repo.linkState).thenReturn(withGuidelines);
        when(() => repo.effectiveGuidelines('mgr')).thenAnswer((_) async => const EffectiveGuidelines(guidelines: [coding], limitChars: 24000, totalChars: 21, employeeId: 'mgr'));
        await open(tester);
        await tester.pumpAndSettle();
        expect(find.text('Guidelines  1', findRichText: true), findsOneWidget);
        expect(find.text('Rust style'), findsOneWidget);
        await tester.tap(find.byKey(const ValueKey('employee-guideline-g1')));
        await tester.pumpAndSettle();
        expect(tester.widget<SelectableText>(find.byType(SelectableText)).data, 'Format with rustfmt.\n');
        expect(tester.takeException(), isNull);
      });

      testWidgets('a failed read says so and tries again', (tester) async {
        when(() => repo.linkState).thenReturn(withGuidelines);
        var calls = 0;
        when(() => repo.effectiveGuidelines('mgr')).thenAnswer((_) async {
          calls++;
          if (calls == 1) throw Exception('offline');
          return const EffectiveGuidelines(guidelines: [coding], limitChars: 24000, totalChars: 21);
        });
        await open(tester);
        await tester.pumpAndSettle();
        expect(find.text("Couldn't load the guidelines"), findsOneWidget);
        await tester.tap(find.byKey(const ValueKey('employee-guidelines-retry')));
        await tester.pumpAndSettle();
        expect(find.text('Rust style'), findsOneWidget);
        expect(calls, 2);
      });

      testWidgets('an older server without guidelines shows no section and is not asked', (tester) async {
        await open(tester);
        expect(find.textContaining('Guidelines', findRichText: true), findsNothing);
        verifyNever(() => repo.effectiveGuidelines(any()));
      });
    });

    group("its own chat's mode, browser and screen (where its group chat answers run)", () {
      List<Map<String, dynamic>> sent() => [for (final c in verify(() => company.send(captureAny(), retry: false)).captured) c as Map<String, dynamic>];

      Future<void> open(WidgetTester tester, EmployeeSummary m) async {
        tester.view.physicalSize = const Size(800, 2400);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        await pump(tester, m);
      }

      testWidgets('the mode is set on its own chat', (tester) async {
        await open(tester, iris());
        expect(find.descendant(of: find.byKey(const ValueKey('employee-mode')), matching: find.text('Ask')), findsOneWidget);
        await tester.tap(find.byKey(const ValueKey('employee-mode')));
        await tester.pumpAndSettle();
        await tester.tap(find.byKey(const ValueKey('employee-mode-auto')));
        await tester.pumpAndSettle();
        final command = sent().single;
        expect(command['method'], 'conversation.set_agent_mode');
        expect(command['params'], {'conversation_id': 'own', 'mode': 'auto'});
      });

      testWidgets('the browser is allowed on its own chat', (tester) async {
        await open(tester, employeeOf(id: 'mgr', name: 'Iris', conversationId: 'own'));
        await tester.tap(find.byKey(const ValueKey('employee-browser')));
        await tester.pump();
        final command = sent().single;
        expect(command['method'], 'conversation.set_browser_allowed');
        expect(command['params'], {'conversation_id': 'own', 'allowed': true});
      });

      testWidgets('the screen asks first, then is allowed on its own chat', (tester) async {
        await open(tester, iris());
        await tester.tap(find.byKey(const ValueKey('employee-screen')));
        await tester.pumpAndSettle();
        expect(find.text('Let Iris see your screen?'), findsOneWidget);
        await tester.tap(find.byKey(const ValueKey('employee-screen-allow')));
        await tester.pumpAndSettle();
        final command = sent().single;
        expect(command['method'], 'conversation.set_screen_allowed');
        expect(command['params'], {'conversation_id': 'own', 'allowed': true});
      });

      testWidgets('turning the screen off asks nothing', (tester) async {
        await open(tester, employeeOf(id: 'mgr', name: 'Iris', conversationId: 'own', screenAllowed: true));
        await tester.tap(find.byKey(const ValueKey('employee-screen')));
        await tester.pumpAndSettle();
        expect(find.text('Let Iris see your screen?'), findsNothing);
        expect(sent().single['params'], {'conversation_id': 'own', 'allowed': false});
      });
    });

    testWidgets('the page tells who the employee reports to and who reports to it', (tester) async {
      final boss = employeeOf(id: 'boss', name: 'Jean');
      final report = employeeOf(id: 'rep', name: 'Dean', reportsTo: 'mgr');
      await pump(tester, iris(reportsTo: 'boss'), team: [boss, report]);
      expect(find.descendant(of: find.byKey(const ValueKey('employee-boss')), matching: find.text('Jean')), findsOneWidget);
      expect(find.text('Team'), findsOneWidget);
      expect(find.text('Dean'), findsOneWidget);
    });

    testWidgets('a root reports to nobody: it is in the management team', (tester) async {
      await pump(tester, iris());
      expect(find.descendant(of: find.byKey(const ValueKey('employee-boss')), matching: find.text('Nobody · management team')), findsOneWidget);
    });

    testWidgets("a task's conversation opens on its own page with the task's title", (tester) async {
      final tasks = [taskOf(id: 'run', title: 'Running task', employeeId: 'mgr', status: TaskStatus.running, conversationId: 'c-run')];
      await pump(tester, iris(conversations: [conversationOf('c-run', title: 'Running')]), tasks: tasks, conversationId: 'c-run');
      verify(() => repo.messages('c-run')).called(1);
      expect(find.text('Running task'), findsOneWidget);
      expect(find.text('Task conversation with Iris'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('a conversation outside a task is one with the employee', (tester) async {
      await pump(tester, iris(), conversationId: 'main');
      verify(() => repo.messages('main')).called(1);
      expect(find.text('Conversation with Iris'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    group('typing while an answer runs', () {
      Future<void> openRunning(WidgetTester tester) async {
        when(() => repo.messages('main')).thenAnswer((_) async => snapshotOf('main', messages: [
              messageOf('u1', MessageRole.user, 'Build it', ordinal: 1),
              messageOf('a1', MessageRole.assistant, 'Working', ordinal: 2, status: MessageStatus.partial),
            ]));
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
            .thenAnswer((_) async => receiptOf(const CommandResultEmployeeMessageSent(conversationId: 'main', messageId: 'm', runId: 'r')));
        await pump(tester, iris(), conversationId: 'main');
        await tester.pump(const Duration(milliseconds: 300));
      }

      List<Map<String, dynamic>> sent() => writesOf(repo);

      testWidgets('with nothing typed one button stops the answer', (tester) async {
        await openRunning(tester);
        expect(find.byKey(const Key('composer-stop')), findsOneWidget);
        expect(find.byKey(const Key('composer-send')), findsNothing);
        expect(find.byIcon(Icons.bolt), findsNothing, reason: 'the separate Send now button is gone');
        await tester.tap(find.byKey(const Key('composer-stop')));
        await tester.pump();
        expect(sent().map((c) => c['method']), ['run.stop']);
        await tester.pump(const Duration(milliseconds: 300));
      });

      testWidgets('typing turns it into Send now: it stops the answer and the message interrupts', (tester) async {
        await openRunning(tester);
        await tester.enterText(find.byType(TextField).last, 'Use Postgres instead');
        await tester.pump();
        expect(find.byKey(const Key('composer-stop')), findsNothing);
        await tester.tap(find.byKey(const Key('composer-send')));
        await tester.pump(const Duration(milliseconds: 100));
        final commands = sent();
        expect(commands.map((c) => c['method']), ['run.stop', 'employee.send_message']);
        expect((commands.last['params'] as Map)['interrupt'], true);
      });

      testWidgets('a long press queues the message behind the answer instead', (tester) async {
        await openRunning(tester);
        await tester.enterText(find.byType(TextField).last, 'And add tests');
        await tester.pump();
        await tester.longPress(find.byKey(const Key('composer-send')));
        await tester.pump(const Duration(milliseconds: 100));
        final commands = sent();
        expect(commands.map((c) => c['method']), ['employee.send_message']);
        expect((commands.single['params'] as Map).containsKey('interrupt'), isFalse);
      });
    });

    testWidgets('an image sent to an employee shows with the message, without an empty bubble', (tester) async {
      when(() => repo.messages('main')).thenAnswer((_) async => snapshotOf('main', messages: [
            messageOf('u1', MessageRole.user, '', files: const [FileRef(id: 'f1', name: 'shot.jpg', mediaType: 'image/jpeg', size: 204800)]),
          ]));
      await pump(tester, iris(), conversationId: 'main');
      expect(find.text('shot.jpg · 200 KB'), findsOneWidget);
      expect(find.byIcon(Icons.image_outlined), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('files sent to an employee show as chips with the message, without an empty bubble', (tester) async {
      when(() => repo.messages('main')).thenAnswer((_) async => snapshotOf('main', messages: [
            messageOf('u1', MessageRole.user, '', uploads: const [MessageFileView(name: 'spec.pdf', path: '.aurelius/uploads/spec.pdf', byteLength: 204800)]),
          ]));
      await pump(tester, iris(), conversationId: 'main');
      expect(find.text('spec.pdf'), findsOneWidget);
      expect(find.text('200 KB'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('an uploaded image shows a thumbnail from the server and opens in the app', (tester) async {
      when(() => repo.messages('main')).thenAnswer((_) async => snapshotOf('main', messages: [
            messageOf('u1', MessageRole.user, '', uploads: const [MessageFileView(id: 'm:u1:0', name: 'shot.png', path: '.aurelius/uploads/shot.png', byteLength: 2048)]),
          ]));
      when(() => repo.uploadsFetchable).thenReturn(true);
      when(() => repo.uploadThumbnail('m:u1:0', any())).thenAnswer((_) async => _png);
      when(() => repo.fetchUpload('m:u1:0')).thenAnswer((_) async => _png);
      await pump(tester, iris(), conversationId: 'main');
      await tester.pump();
      expect(find.byKey(const Key('upload-thumb-m:u1:0')), findsOneWidget);
      await tester.tap(find.byKey(const Key('upload-m:u1:0')));
      await tester.pump();
      await tester.pump();
      expect(find.byKey(const Key('upload-image')), findsOneWidget);
      verify(() => repo.fetchUpload('m:u1:0')).called(1);
      expect(tester.takeException(), isNull);
    });

    testWidgets('an older server keeps uploaded files as plain chips', (tester) async {
      when(() => repo.messages('main')).thenAnswer((_) async => snapshotOf('main', messages: [
            messageOf('u1', MessageRole.user, '', uploads: const [MessageFileView(id: 'm:u1:0', name: 'shot.png', path: '.aurelius/uploads/shot.png', byteLength: 2048)]),
          ]));
      when(() => repo.uploadsFetchable).thenReturn(false);
      await pump(tester, iris(), conversationId: 'main');
      expect(find.text('shot.png'), findsOneWidget);
      expect(find.byKey(const Key('upload-m:u1:0')), findsNothing);
      verifyNever(() => repo.uploadThumbnail(any(), any()));
      expect(tester.takeException(), isNull);
    });

    testWidgets("Claude's question stands in the conversation, with its options, to answer from there", (tester) async {
      when(() => repo.messages('main')).thenAnswer((_) async => snapshotOf('main', messages: [
            messageOf('q', MessageRole.assistant, '**Tone** · Which tone?\n\n- **Formal** — Like a bank\n- **Playful** — Like a game', author: 'Iris', update: 'approval', approvalId: 'q1'),
          ]));
      final question = approvalOf(
        'q1',
        tool: ApprovalViewState.questionTool,
        effect: 'read',
        requester: 'Iris',
        conversationId: 'main',
        arguments: {
          'questions': [
            {'question': 'Which tone?', 'header': 'Tone', 'multiSelect': false, 'options': []},
          ],
        },
      );
      await pump(tester, iris(), inboxState: InboxState(loading: false, approvals: [question]), conversationId: 'main');
      expect(find.text('QUESTION · WAITING FOR YOU · Iris'), findsOneWidget);
      expect(find.textContaining('Like a game'), findsOneWidget);
      expect(find.widgetWithText(FilledButton, 'Answer'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('a mode the manager chose is its own line with the reason, not a generic update', (tester) async {
      when(() => repo.messages('main')).thenAnswer((_) async => snapshotOf('main', messages: [
            messageOf('m', MessageRole.assistant, 'Bo chose Auto: small fix, easy to undo', author: 'Bo', update: 'mode_chosen'),
          ]));
      await pump(tester, iris(), conversationId: 'main');
      expect(find.text('MODE CHOSEN · Bo'), findsOneWidget);
      expect(find.text('UPDATE · Bo'), findsNothing);
      expect(find.textContaining('chose Auto: small fix'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('the subagent chip counts the subagents of this conversation; a tap shows what they do', (tester) async {
      final now = DateTime.now().millisecondsSinceEpoch;
      final busy = iris(agents: [
        AgentView(kind: 'conversation', name: 'Answering', detail: '', conversationId: 'main', sinceMs: now, steps: 0),
        subagent('Find the chip', 'main', now - 72000, detail: 'Explore', prompt: 'Look for the chip.\nIn chat_view.\nAnd git_chip.\nThen report.', doing: 'Grep · GitChip', steps: 3),
        subagent('Read docs', 'main', now),
        subagent('Elsewhere', 'side', now),
      ]);
      // The server tells a conversation's own subagents.
      when(() => repo.subagents('main')).thenAnswer((_) async => subagentsIn(busy.agents, 'main'));
      await pump(tester, busy, conversationId: 'main');
      expect(find.text('2 subagents'), findsOneWidget);

      await tester.tap(find.byKey(const Key('subagent-chip')));
      // Not pumpAndSettle: the dot pulses and the times tick.
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));
      expect(find.text('Find the chip'), findsOneWidget);
      expect(find.text('Elsewhere'), findsNothing);
      expect(find.textContaining(RegExp(r'^1m 1\ds$')), findsOneWidget, reason: 'the real clock runs on during the test');
      expect(find.text('↳ Grep · GitChip · 3 steps'), findsOneWidget);
      expect(find.text('↳ starting…'), findsOneWidget);
      expect(find.text('Look for the chip.\nIn chat_view.\nAnd git_chip. …'), findsOneWidget);
      await tester.tap(find.text('Look for the chip.\nIn chat_view.\nAnd git_chip. …'));
      await tester.pump();
      expect(find.text('Look for the chip.\nIn chat_view.\nAnd git_chip.\nThen report.'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('the subagent chip asks again when an append shows a new step, at most once a second', (tester) async {
      final appends = StreamController<StreamAppend>.broadcast();
      addTearDown(appends.close);
      when(() => repo.appends).thenAnswer((_) => appends.stream);
      final now = DateTime.now().millisecondsSinceEpoch;
      var agents = [subagent('Find the chip', 'main', now)];
      when(() => repo.subagents('main')).thenAnswer((_) async => agents);
      await pump(tester, iris(), conversationId: 'main');
      expect(find.text('1 subagent'), findsOneWidget);
      verify(() => repo.subagents('main')).called(1);

      ActivityView step(String id, {bool? ok}) => ActivityView(callId: id, tool: 'Grep', input: const {}, offset: 0, ok: ok);
      StreamAppend append(List<ActivityView>? activity, {String conversation = 'main'}) =>
          StreamAppend(runId: 'r', conversationId: conversation, messageId: 'm', offset: 0, text: '', activity: activity);

      // A new step: asks at once.
      agents = [...agents, subagent('Read docs', 'main', now)];
      appends.add(append([step('a')]));
      await tester.pump();
      expect(find.text('2 subagents'), findsOneWidget);
      verify(() => repo.subagents('main')).called(1);

      // Text only, the same steps, or another conversation: no ask.
      appends
        ..add(append(null))
        ..add(append([step('a')]))
        ..add(append([step('b'), step('c')], conversation: 'side'));
      await tester.pump(SubagentChip.settle * 2);
      verifyNever(() => repo.subagents('main'));

      // Two changes within a second: one ask now, one when the second is over.
      appends
        ..add(append([step('a', ok: true)]))
        ..add(append([step('a', ok: true), step('b')]));
      await tester.pump();
      verify(() => repo.subagents('main')).called(1);
      await tester.pump(SubagentChip.settle ~/ 2);
      verifyNever(() => repo.subagents('main'));
      await tester.pump(SubagentChip.settle);
      verify(() => repo.subagents('main')).called(1);
      await tester.pump(SubagentChip.settle * 2);
      verifyNever(() => repo.subagents('main'));
      expect(tester.takeException(), isNull);
    });
  });

}
