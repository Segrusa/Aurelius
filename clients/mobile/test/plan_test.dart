import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/commands.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/chat_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/inbox/plan_review.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

class MockInboxBloc extends MockBloc<InboxEvent, InboxState> implements InboxBloc {}


const online = LinkState(status: LinkStatus.online);

const planText = '# Create hello.txt\n\n1. Write the file\n2. Say DONE';

ApprovalView plan({String id = 'plan-1', int minutes = 30, String text = planText}) => approvalOf(
      id,
      tool: 'claude.ExitPlanMode',
      effect: 'read',
      arguments: {'plan': text, 'planFilePath': 'C:/plans/p.md'},
      expiresInMinutes: minutes,
      requester: 'Alex',
    );

Receipt consumed() => receiptOf(const CommandResultApprovalDecided(approvalId: 'plan-1', ok: true, output: '', status: ApprovalStatus.consumed));

void main() {
  setUpAll(() {
    registerFallbackValue(<String, dynamic>{});
    registerFallbackValue(const InboxRefreshed());
  });

  group('plan approval model and commands', () {
    test('a plan approval carries its markdown, headline and wire commands', () {
      final a = plan();
      expect(a.isPlan, isTrue);
      expect(a.isQuestion, isFalse);
      expect(a.planMarkdown, planText);
      expect(a.planHeadline, 'Create hello.txt');
      expect(a.title, 'Alex has a plan');
      expect(plan(text: '').planHeadline, 'A plan');
      expect(Commands.decidePlan('p', PlanDecision.approve), {'method': 'approval.decide_plan', 'params': {'approval_id': 'p', 'decision': 'approve'}});
      expect(Commands.decidePlan('p', PlanDecision.revise, feedback: 'More tests'), {
        'method': 'approval.decide_plan',
        'params': {'approval_id': 'p', 'decision': 'revise', 'feedback': 'More tests'},
      });
      expect(paramsOf(Commands.decidePlan('p', PlanDecision.reject))['decision'], 'reject');
      expect(paramsOf(Commands.decidePlan('p', PlanDecision.approveEdits))['decision'], 'approve_edits');
      expect(Commands.setBrowserAllowed('c', true), {'method': 'conversation.set_browser_allowed', 'params': {'allowed': true, 'conversation_id': 'c'}});
    });

    test('the browser approvals read as what they are', () {
      final a = approvalOf('x', tool: 'claude.mcp__claude-in-chrome__navigate', requester: 'Alex', arguments: {});
      expect(a.isBrowser, isTrue);
      expect(a.title, 'Alex wants to use your browser');
    });
  });

  group('InboxBloc plans', () {
    late MockRepository repo;

    setUp(() {
      repo = MockRepository();
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.linkState).thenReturn(online);
      when(() => repo.notifications()).thenAnswer((_) async => const NotificationsSnapshot(notifications: [], unread: 0));
      when(() => repo.proposals()).thenAnswer((_) async => []);
    });

    blocTest<InboxBloc, InboxState>(
      'approving sends the decision and remembers how the plan ended',
      build: () {
        when(() => repo.approvals()).thenAnswer((_) async => [plan()]);
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async => consumed());
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await stateWhere(bloc, (s) => !s.loading);
        bloc.add(PlanDecided(bloc.state.approvals.single, PlanDecision.approve));
        await stateWhere(bloc, (s) => s.planOutcomes['plan-1'] == PlanOutcome.approved && s.deciding.isEmpty);
      },
      verify: (bloc) {
        expect(verify(() => repo.write(captureAny(), retry: false)).captured.single, Commands.decidePlan('plan-1', PlanDecision.approve));
        expect(bloc.state.planOutcomes['plan-1'], PlanOutcome.approved);
        expect(bloc.state.deciding, isEmpty);
      },
    );

    blocTest<InboxBloc, InboxState>(
      'approving with edits is remembered as such',
      build: () {
        when(() => repo.approvals()).thenAnswer((_) async => [plan()]);
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async => consumed());
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await stateWhere(bloc, (s) => !s.loading);
        bloc.add(PlanDecided(bloc.state.approvals.single, PlanDecision.approveEdits));
        await stateWhere(bloc, (s) => s.planOutcomes['plan-1'] == PlanOutcome.approvedEdits);
      },
      verify: (bloc) {
        expect(verify(() => repo.write(captureAny(), retry: false)).captured.single, Commands.decidePlan('plan-1', PlanDecision.approveEdits));
        expect(bloc.state.planOutcomes['plan-1'], PlanOutcome.approvedEdits);
      },
    );

    blocTest<InboxBloc, InboxState>(
      'asking for changes without words sends nothing',
      build: () {
        when(() => repo.approvals()).thenAnswer((_) async => [plan()]);
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await stateWhere(bloc, (s) => !s.loading);
        bloc.add(PlanDecided(bloc.state.approvals.single, PlanDecision.revise, feedback: '   '));
        await stateWhere(bloc, (s) => s.notice == 'Say what should change.');
      },
      verify: (bloc) {
        verifyNever(() => repo.write(any(), retry: any(named: 'retry')));
        expect(bloc.state.planOutcomes, isEmpty);
      },
      expect: () => contains(isA<InboxState>().having((s) => s.notice, 'notice', 'Say what should change.')),
    );

    blocTest<InboxBloc, InboxState>(
      'changes carry the feedback, trimmed',
      build: () {
        when(() => repo.approvals()).thenAnswer((_) async => [plan()]);
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async => receiptOf(const CommandResultApprovalDecided(approvalId: 'plan-1', ok: true, output: '', status: ApprovalStatus.rejected)));
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await stateWhere(bloc, (s) => !s.loading);
        bloc.add(PlanDecided(bloc.state.approvals.single, PlanDecision.revise, feedback: '  Add tests  '));
        await stateWhere(bloc, (s) => s.planOutcomes['plan-1'] == PlanOutcome.revisionRequested);
      },
      verify: (bloc) {
        expect(verify(() => repo.write(captureAny(), retry: false)).captured.single, Commands.decidePlan('plan-1', PlanDecision.revise, feedback: 'Add tests'));
        expect(bloc.state.planOutcomes['plan-1'], PlanOutcome.revisionRequested);
      },
    );

    blocTest<InboxBloc, InboxState>(
      'a plan handled on another device says so and changes nothing here',
      build: () {
        when(() => repo.approvals()).thenAnswer((_) async => [plan()]);
        when(() => repo.write(any(), retry: false)).thenThrow(const ProtocolException(ErrorCode.validation, 'This approval was already decided.'));
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await stateWhere(bloc, (s) => !s.loading);
        bloc.add(PlanDecided(bloc.state.approvals.single, PlanDecision.approve));
        await stateWhere(bloc, (s) => s.planOutcomes['plan-1'] == PlanOutcome.handledElsewhere);
      },
      verify: (bloc) {
        expect(bloc.state.planOutcomes['plan-1'], PlanOutcome.handledElsewhere);
        expect(bloc.state.deciding, isEmpty);
      },
      expect: () => contains(isA<InboxState>().having((s) => s.notice, 'notice', 'This plan was already handled.')),
    );

    blocTest<InboxBloc, InboxState>(
      'an unreachable PC decides nothing and the plan stays open',
      build: () {
        when(() => repo.approvals()).thenAnswer((_) async => [plan()]);
        when(() => repo.write(any(), retry: false)).thenThrow(const NotSentException('cmd-1'));
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await stateWhere(bloc, (s) => !s.loading);
        bloc.add(PlanDecided(bloc.state.approvals.single, PlanDecision.reject));
        await stateWhere(bloc, (s) => s.notice?.startsWith('Not sent.') ?? false);
      },
      verify: (bloc) {
        expect(bloc.state.planOutcomes, isEmpty);
        expect(bloc.state.approvals, hasLength(1));
      },
      expect: () => contains(isA<InboxState>().having((s) => s.notice, 'notice', startsWith('Not sent.'))),
    );
  });

  group('ChatBloc new project conversation', () {
    late MockRepository repo;

    setUp(() {
      repo = MockRepository();
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.linkState).thenReturn(online);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'))).thenAnswer((_) async => receiptOf(const CommandResultProjectMessageSent(conversationId: 'new-1', messageId: 'm', projectId: 'p', runId: 'r')));
      when(() => repo.write(any(), retry: false)).thenAnswer((_) async => receiptOf(const CommandResultBrowserAllowedSet(allowed: true, conversationId: 'new-1')));
    });

    test('the switches are set before the first message and start the conversation with it', () async {
      when(() => repo.messages('new-1')).thenAnswer((_) async => snapshotOf('new-1', agentMode: AgentMode.plan, browserAllowed: true));
      final bloc = ChatBloc(repo, projectId: 'p');
      bloc
        ..add(const ChatBrowserToggled())
        ..add(const ChatModeSelected(AgentMode.plan));
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(bloc.state.notice, isNull, reason: 'no "send a first message" any more');
      expect((bloc.state.browserAllowed, bloc.state.mode), (true, AgentMode.plan));
      verifyNever(() => repo.write(any(), retry: false));

      bloc.add(const ChatMessageSent('Wie heeft de Tour gewonnen?'));
      await Future<void>.delayed(const Duration(milliseconds: 100));
      final sent = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'))).captured.single as Map<String, dynamic>;
      expect(sent['method'], 'project_conversation.send');
      expect(paramsOf(sent), containsPair('agent_mode', 'plan'));
      expect(paramsOf(sent), containsPair('browser_allowed', true));
      expect(paramsOf(sent).containsKey('screen_allowed'), isFalse);
      expect(bloc.state.conversationId, 'new-1');
      verifyNever(() => repo.write(any(), retry: false));
      await bloc.close();
    });
  });

  group('ChatBloc browser', () {
    late MockRepository repo;

    setUp(() {
      repo = MockRepository();
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.linkState).thenReturn(online);
      when(() => repo.messages('conv-1')).thenAnswer((_) async => snapshotOf('conv-1'));
    });

    blocTest<ChatBloc, ChatState>(
      'the browser is off, turns on with the command, and the PC can refuse',
      build: () => ChatBloc(repo, conversationId: 'conv-1', projectId: 'p'),
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 100));
        expect(bloc.state.browserAllowed, isFalse);
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async => receiptOf(const CommandResultBrowserAllowedSet(allowed: true, conversationId: 'conv-1')));
        bloc.add(const ChatBrowserToggled());
        await Future<void>.delayed(const Duration(milliseconds: 100));
        expect(bloc.state.browserAllowed, isTrue);
        expect(verify(() => repo.write(captureAny(), retry: false)).captured.single, Commands.setBrowserAllowed('conv-1', true));
        // Turning it off again works; then a refusal puts the switch back.
        bloc.add(const ChatBrowserToggled());
        await Future<void>.delayed(const Duration(milliseconds: 100));
        expect(bloc.state.browserAllowed, isFalse);
        when(() => repo.write(any(), retry: false)).thenThrow(const ProtocolException(ErrorCode.forbidden, 'Needs the Approve right.'));
        bloc.add(const ChatBrowserToggled());
        await Future<void>.delayed(const Duration(milliseconds: 100));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        expect(bloc.state.browserAllowed, isFalse, reason: 'a refused switch goes back');
        expect(bloc.state.notice, 'Needs the Approve right.');
      },
    );
  });

  group('PlanReview sheet', () {
    late MockInboxBloc inbox;
    late MockConnectionBloc connection;

    Future<void> pump(WidgetTester tester, ApprovalView approval, InboxState state, {double keyboard = 0, LinkState link = online}) async {
      inbox = MockInboxBloc();
      whenListen(inbox, const Stream<InboxState>.empty(), initialState: state);
      connection = MockConnectionBloc();
      whenListen(connection, const Stream<LinkState>.empty(), initialState: link);
      tester.view.physicalSize = const Size(800, 1200);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(MaterialApp(
        theme: aureliusTheme(),
        home: MultiBlocProvider(
          providers: [BlocProvider<InboxBloc>.value(value: inbox), BlocProvider<ConnectionBloc>.value(value: connection)],
          child: Builder(
            builder: (context) => MediaQuery(
              data: MediaQuery.of(context).copyWith(viewInsets: EdgeInsets.only(bottom: keyboard)),
              child: Scaffold(body: PlanReview(approval: approval)),
            ),
          ),
        ),
      ));
      await tester.pump(const Duration(milliseconds: 300));
    }

    FloatingActionButton fab(WidgetTester tester, String key) =>
        tester.widget<FloatingActionButton>(find.descendant(of: find.byKey(Key(key)), matching: find.byType(FloatingActionButton)));

    testWidgets('shows the plan as markdown and offers the four choices', (tester) async {
      final approval = plan();
      await pump(tester, approval, InboxState(loading: false, approvals: [approval]));
      expect(find.text('Create hello.txt'), findsOneWidget, reason: 'the heading is rendered, not shown as "# ..."');
      expect(find.textContaining('# Create'), findsNothing);
      expect(find.text('Write the file'), findsOneWidget);
      expect(find.byType(FloatingActionButton), findsNWidgets(4));
      expect(fab(tester, 'plan-approve').onPressed, isNotNull);
      expect(fab(tester, 'plan-approve-edits').onPressed, isNotNull);
      expect(fab(tester, 'plan-reject').onPressed, isNotNull, reason: 'rejecting needs no reason');
      expect(fab(tester, 'plan-changes').onPressed, isNull, reason: 'changes need feedback');
      expect(find.byKey(const Key('plan-status')), findsNothing);
    });

    testWidgets('a long plan scrolls while the buttons stay where they are', (tester) async {
      final long = List.generate(80, (i) => '- step $i').join('\n');
      final approval = plan(text: '# Big plan\n\n$long');
      await pump(tester, approval, InboxState(loading: false, approvals: [approval]));
      final buttonTop = tester.getTopLeft(find.byKey(const Key('plan-approve'))).dy;
      final planArea = tester.getRect(find.byKey(const Key('plan-text')));
      // Markdown builds every line, so "visible" means: inside the plan area.
      bool inView(String text) => planArea.contains(tester.getCenter(find.text(text, skipOffstage: false)));
      expect(inView('step 0'), isTrue);
      expect(inView('step 79'), isFalse, reason: 'below the plan area until scrolled to');
      await tester.dragUntilVisible(find.text('step 79'), find.byKey(const Key('plan-text')), const Offset(0, -300));
      await tester.pump();
      expect(inView('step 79'), isTrue);
      expect(inView('step 0'), isFalse);
      expect(tester.getTopLeft(find.byKey(const Key('plan-approve'))).dy, buttonTop);
      expect(fab(tester, 'plan-approve').onPressed, isNotNull);
    });

    testWidgets('approving sends no feedback, even when some was typed', (tester) async {
      final approval = plan();
      await pump(tester, approval, InboxState(loading: false, approvals: [approval]));
      await tester.enterText(find.byKey(const Key('plan-feedback')), 'Half-written thought');
      await tester.pump();
      await tester.tap(find.byKey(const Key('plan-approve')));
      final event = verify(() => inbox.add(captureAny())).captured.single as PlanDecided;
      expect((event.decision, event.feedback), (PlanDecision.approve, null));
    });

    testWidgets('approve sends the decision', (tester) async {
      final approval = plan();
      await pump(tester, approval, InboxState(loading: false, approvals: [approval]));
      await tester.tap(find.byKey(const Key('plan-approve')));
      final event = verify(() => inbox.add(captureAny())).captured.single as PlanDecided;
      expect(event.decision, PlanDecision.approve);
      expect(event.approval.id, 'plan-1');
      expect(event.feedback, isNull);
    });

    testWidgets('approve with edits sends that decision, without feedback', (tester) async {
      final approval = plan();
      await pump(tester, approval, InboxState(loading: false, approvals: [approval]));
      await tester.enterText(find.byKey(const Key('plan-feedback')), 'Half-written thought');
      await tester.pump();
      await tester.tap(find.byKey(const Key('plan-approve-edits')));
      final event = verify(() => inbox.add(captureAny())).captured.single as PlanDecided;
      expect((event.decision, event.feedback), (PlanDecision.approveEdits, null));
    });

    testWidgets('changes unlock with feedback and send it; reject may carry feedback too', (tester) async {
      final approval = plan();
      await pump(tester, approval, InboxState(loading: false, approvals: [approval]));
      await tester.enterText(find.byKey(const Key('plan-feedback')), '  Also add a README ');
      await tester.pump();
      expect(fab(tester, 'plan-changes').onPressed, isNotNull);
      await tester.tap(find.byKey(const Key('plan-changes')));
      var event = verify(() => inbox.add(captureAny())).captured.single as PlanDecided;
      expect((event.decision, event.feedback), (PlanDecision.revise, 'Also add a README'));
      await tester.tap(find.byKey(const Key('plan-reject')));
      event = verify(() => inbox.add(captureAny())).captured.single as PlanDecided;
      expect((event.decision, event.feedback), (PlanDecision.reject, 'Also add a README'));
    });

    testWidgets('after a choice the sheet says what happened and the buttons are off', (tester) async {
      final approval = plan();
      await pump(tester, approval, InboxState(loading: false, approvals: const [], planOutcomes: const {'plan-1': PlanOutcome.approved}));
      expect(find.text('Approved. The worker goes on in auto mode.'), findsOneWidget);
      expect([for (final k in ['plan-approve', 'plan-approve-edits', 'plan-reject']) fab(tester, k).onPressed], everyElement(isNull));
      expect(find.byKey(const Key('plan-changes')), findsNothing, reason: 'the quick revision is only offered while the plan is open');
      expect(find.byKey(const Key('plan-feedback')), findsNothing);
      expect(find.text('Write the file'), findsOneWidget, reason: 'the plan stays readable');
    });

    testWidgets('a plan handled on another device is read-only', (tester) async {
      final approval = plan();
      // Not in the inbox any more, and not decided here.
      await pump(tester, approval, const InboxState(loading: false, approvals: []));
      expect(find.text('Already handled on another device.'), findsOneWidget);
      expect([for (final k in ['plan-approve', 'plan-approve-edits', 'plan-reject']) fab(tester, k).onPressed], everyElement(isNull));
      expect(find.byKey(const Key('plan-changes')), findsNothing, reason: 'the quick revision is only offered while the plan is open');
    });

    testWidgets('while a decision is on its way, and when the PC is unreachable, nothing can be pressed', (tester) async {
      final approval = plan();
      await pump(tester, approval, InboxState(loading: false, approvals: [approval], deciding: const {'plan-1'}));
      expect(fab(tester, 'plan-approve').onPressed, isNull);
      expect(fab(tester, 'plan-reject').onPressed, isNull);
      await pump(tester, approval, InboxState(loading: false, approvals: [approval]), link: const LinkState(status: LinkStatus.offline));
      expect(fab(tester, 'plan-approve').onPressed, isNull);
      expect(fab(tester, 'plan-reject').onPressed, isNull);
    });

    testWidgets('an expired plan cannot be approved', (tester) async {
      final approval = plan(minutes: -1);
      await pump(tester, approval, InboxState(loading: false, approvals: [approval]));
      expect(find.text('Expired. Expiry never counts as permission.'), findsOneWidget);
      expect(fab(tester, 'plan-approve').onPressed, isNull);
    });

    testWidgets('the feedback field and buttons ride above the keyboard', (tester) async {
      final approval = plan();
      final state = InboxState(loading: false, approvals: [approval]);
      await pump(tester, approval, state);
      final fieldBefore = tester.getBottomLeft(find.byKey(const Key('plan-feedback'))).dy;
      final buttonBefore = tester.getBottomLeft(find.byKey(const Key('plan-approve'))).dy;
      await pump(tester, approval, state, keyboard: 400);
      final fieldAfter = tester.getBottomLeft(find.byKey(const Key('plan-feedback'))).dy;
      final buttonAfter = tester.getBottomLeft(find.byKey(const Key('plan-approve'))).dy;
      expect(buttonBefore - buttonAfter, closeTo(400, 1));
      expect(fieldBefore - fieldAfter, closeTo(400, 1));
      expect(buttonAfter, lessThanOrEqualTo(1200 - 400));
      expect(tester.takeException(), isNull);
    });
  });

  group('conversations that need you', () {
    ApprovalView tool(String name, {String conversation = 'conv-1'}) => approvalOf(name, tool: name, requester: 'Alex', arguments: {}, conversationId: conversation);

    test('a question beats a plan, a plan beats an approval', () {
      expect(const InboxState(approvals: []).needsYou('conv-1'), isNull);
      expect(InboxState(approvals: [tool('fs.write')]).needsYou('conv-1'), 'Approve');
      expect(InboxState(approvals: [tool('fs.write')]).needsYou('conv-2'), isNull);
      expect(InboxState(approvals: [tool('fs.write'), plan()]).needsYou('conv-1'), 'Review plan');
      expect(InboxState(approvals: [plan(), tool('claude.AskUserQuestion')]).needsYou('conv-1'), 'Answer');
    });
  });

  test('composePlanFeedback quotes fragments, then the free text', () {
    expect(composePlanFeedback([(quote: 'a\nb', text: 'Split')], ' more '), '> a\n> b\n\nSplit\n\nmore');
    expect(composePlanFeedback([], '  '), '');
  });
}
