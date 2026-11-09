import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/commands.dart';
import 'package:aurelius_mobile/data/secret_write.dart';
import 'package:aurelius_mobile/data/vault.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/new_ticket_page.dart';
import 'package:aurelius_mobile/features/board/ticket_actions.dart';
import 'package:aurelius_mobile/features/chat/chat_bloc.dart';
import 'package:aurelius_mobile/features/chat/secret_check.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

// B10 after AUR-MU-04-F5 (M4): every text command a board member sends from
// the phone that takes `confirm_secret` asks with the dialog when refused as
// holding a vault value, sends it once more with `confirm_secret` on Send
// anyway, and a second refusal ends with a notice, never a loop. Neutral
// names and a fake secret only.

const fakeSecret = 'sk-test-FAKE-0000';
const refused = ProtocolException(ErrorCode.looksLikeSecret, 'This looks like a vault secret.', matchedItems: ['GitHub token']);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  void refuseAll(MockRepository repo) =>
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenThrow(refused);

  /// The writes of [repo] with their idempotency keys and expected versions, in order.
  List<(Map<String, dynamic>, String?, int?)> sent(MockRepository repo) {
    final c = verify(() => repo.write(captureAny(), idempotencyKey: captureAny(named: 'idempotencyKey'), expectedVersion: captureAny(named: 'expectedVersion'), retry: any(named: 'retry'))).captured;
    return [for (var i = 0; i < c.length; i += 3) (c[i] as Map<String, dynamic>, c[i + 1] as String?, c[i + 2] as int?)];
  }

  final dialog = find.byKey(const ValueKey('secret-check'));
  final sendAnyway = find.byKey(const ValueKey('secret-check-send'));

  /// The three checks of M4 on [method], after the first tap: the dialog with
  /// only the names, Send anyway with `confirm_secret`, refused again a notice
  /// ([noticeShown]) and no second dialog.
  Future<void> threeChecks(WidgetTester tester, MockRepository repo, String method, {required Future<void> Function() settle, required void Function() noticeShown, int? expectedVersion}) async {
    expect(dialog, findsOneWidget, reason: 'a refusal asks with the dialog');
    expect(find.descendant(of: dialog, matching: find.text('GitHub token')), findsOneWidget);
    expect(find.descendant(of: dialog, matching: find.textContaining(fakeSecret)), findsNothing, reason: 'never the value');
    expect(sendAnyway, findsOneWidget, reason: '$method takes confirm_secret');
    await tester.tap(sendAnyway);
    await settle();
    final writes = sent(repo);
    expect(writes.map((w) => w.$1['method']), [method, method]);
    expect(paramsOf(writes[0].$1).containsKey('confirm_secret'), isFalse);
    expect(paramsOf(writes[1].$1)['confirm_secret'], isTrue, reason: 'Send anyway sends it again with confirm_secret');
    expect({...paramsOf(writes[1].$1)}..remove('confirm_secret'), paramsOf(writes[0].$1), reason: 'the same text, nothing else changed');
    expect(writes[1].$2 == null || writes[1].$2 != writes[0].$2, isTrue, reason: 'a new request, so a new key');
    expect(writes[1].$3, expectedVersion);
    expect(dialog, findsNothing, reason: 'refused again: no second question, so no loop');
    noticeShown();
    await tester.pump(const Duration(milliseconds: 200));
    // Nothing more than the two above (mocktail's verify only sees calls not verified yet).
    verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
  }

  group('writeConfirmingSecret', () {
    test('a command without confirm_secret (ticket.approve_plan) only says what to take out and is sent once', () async {
      final repo = MockRepository();
      refuseAll(repo);
      final asked = <bool>[];
      final receipt = await writeConfirmingSecret(repo, ticketApprovePlan(ticketId: 'k1', decision: PlanApproval.approve, feedback: 'Use $fakeSecret'), (items, {required canSendAnyway}) async {
        asked.add(canSendAnyway);
        return true;
      });
      expect(receipt, isNull);
      expect(asked, [false]);
      expect(sent(repo), hasLength(1));
    });

    test("Don't send: nothing is sent again", () async {
      final repo = MockRepository();
      refuseAll(repo);
      final receipt = await writeConfirmingSecret(repo, ticketSendBack(ticketId: 'k1', feedback: 'x'), (items, {required canSendAnyway}) async => false);
      expect(receipt, isNull);
      expect(sent(repo), hasLength(1));
    });

    test('another error is not asked about', () async {
      final repo = MockRepository();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenThrow(const ProtocolException(ErrorCode.validation, 'Too long.'));
      var asked = false;
      await expectLater(
        writeConfirmingSecret(repo, ticketSendBack(ticketId: 'k1', feedback: 'x'), (items, {required canSendAnyway}) async => asked = true),
        throwsA(isA<ProtocolException>().having((e) => e.message, 'message', 'Too long.')),
      );
      expect(asked, isFalse);
    });

    test('the F5 commands the phone sends from a text field all take confirm_secret; names, memories and approve_plan do not', () {
      for (final request in [
        Commands.createTask('p1', 't', 'g', const []),
        Commands.sendProjectMessage('p1', 'x'),
        Commands.addIdea('p1', 't', null),
        Commands.updateIdea('i1', 't', null),
        Commands.answerQuestion('a1', const {'q': 'x'}),
        Commands.decidePlan('a1', PlanDecision.revise, feedback: 'x'),
        ticketCreate(projectId: 'p1', title: 't'),
        ticketUpdate(ticketId: 'k1', title: 't'),
        ticketRevisePlan(ticketId: 'k1', feedback: 'x'),
        ticketSendBack(ticketId: 'k1', feedback: 'x'),
        ticketAccept(ticketId: 'k1'),
      ]) {
        expect(takesConfirmSecret(request), isTrue, reason: '${request['method']}');
      }
      for (final request in [
        Commands.createEmployee('Alex', 'Dev'),
        Commands.remember('global', null, MemoryKind.decision, 'x'),
        Commands.createProject('Atlas'),
        ticketApprovePlan(ticketId: 'k1', decision: PlanApproval.approve),
      ]) {
        expect(takesConfirmSecret(request), isFalse, reason: '${request['method']}');
      }
      expect(paramsOf(Commands.sendProjectMessage('p1', 'x', confirmSecret: true))['confirm_secret'], isTrue);
      expect(paramsOf(Commands.sendProjectMessage('p1', 'x')).containsKey('confirm_secret'), isFalse);
    });
  });

  group('project chat (project_conversation.send)', () {
    MockRepository chatRepository() {
      final repo = boardRepository();
      when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.messages(any())).thenAnswer((_) async => snapshotOf('c1'));
      return repo;
    }

    test('refused: the question; Send anyway with confirm_secret and a new key; refused again a notice and no second question', () async {
      final repo = chatRepository();
      refuseAll(repo);
      final bloc = ChatBloc(repo, conversationId: 'c1', projectId: 'p1');
      await stateWhere(bloc, (s) => !s.loading);
      bloc.add(const ChatMessageSent('The token is $fakeSecret'));
      final asked = await stateWhere(bloc, (s) => s.secretCheck != null);
      expect(asked.unsent.single.secretItems, ['GitHub token']);
      expect(asked.notice, isNull, reason: 'the question, not the old "take it out" notice');
      bloc.add(const ChatSecretCheckShown());
      await stateWhere(bloc, (s) => s.secretCheck == null);
      bloc.add(ChatSecretConfirmed(asked.unsent.single.localId));
      final ended = await stateWhere(bloc, (s) => s.notice != null);
      expect(ended.notice, secretCheckNoSendAnyway);
      expect(ended.secretCheck, isNull, reason: 'no new question, so no loop');
      expect(ended.unsent.single.failed, isTrue);
      await Future<void>.delayed(const Duration(milliseconds: 50));
      final writes = sent(repo);
      expect(writes.map((w) => w.$1['method']), ['project_conversation.send', 'project_conversation.send']);
      expect(paramsOf(writes[0].$1).containsKey('confirm_secret'), isFalse);
      expect(paramsOf(writes[1].$1)['confirm_secret'], isTrue);
      expect(writes[1].$2, isNot(writes[0].$2));
      await bloc.close();
    });

    test('Send anyway that goes through delivers it', () async {
      final repo = chatRepository();
      var calls = 0;
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenAnswer((_) async {
        if (calls++ == 0) throw refused;
        return receiptOf(const CommandResultProjectMessageSent(conversationId: 'c1', messageId: 'm1', projectId: 'p1', runId: 'r1'));
      });
      final bloc = ChatBloc(repo, conversationId: 'c1', projectId: 'p1');
      await stateWhere(bloc, (s) => !s.loading);
      bloc.add(const ChatMessageSent('The token is $fakeSecret'));
      final asked = await stateWhere(bloc, (s) => s.secretCheck != null);
      bloc.add(ChatSecretConfirmed(asked.unsent.single.localId));
      await stateWhere(bloc, (s) => s.unsent.isEmpty || s.unsent.single.delivered);
      expect(calls, 2);
      await bloc.close();
    });
  });

  group('the ticket writes (ticketWrite)', () {
    /// A button that runs [write] with the page's context, as the ticket pages do.
    Future<List<bool>> pumpWriter(WidgetTester tester, MockRepository repo, Future<bool> Function(BuildContext) write) async {
      final results = <bool>[];
      await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
        value: repo,
        child: MaterialApp(
          theme: aureliusTheme(),
          home: Scaffold(body: Builder(builder: (context) => TextButton(onPressed: () async => results.add(await write(context)), child: const Text('go')))),
        ),
      ));
      await tester.tap(find.text('go'));
      await tester.pumpAndSettle();
      return results;
    }

    for (final (name, request) in [
      ('ticket.update (title)', ticketUpdate(ticketId: 'k1', title: 'Deploy with $fakeSecret')),
      ('ticket.update (description)', ticketUpdate(ticketId: 'k1', description: 'Token $fakeSecret')),
      ('ticket.revise_plan (Pas aan)', ticketRevisePlan(ticketId: 'k1', feedback: 'Use $fakeSecret')),
      ('ticket.send_back (Send back)', ticketSendBack(ticketId: 'k1', feedback: 'Use $fakeSecret')),
    ]) {
      testWidgets('$name: dialog, Send anyway with confirm_secret, refused again a toast without a loop', (tester) async {
        final repo = boardRepository();
        refuseAll(repo);
        final results = await pumpWriter(tester, repo, (context) => ticketWrite(context, request, expectedVersion: 3, retry: false, done: 'Done.'));
        await threeChecks(
          tester,
          repo,
          request['method'] as String,
          expectedVersion: 3,
          settle: () => tester.pumpAndSettle(),
          noticeShown: () => expect(find.text(secretCheckNoSendAnyway), findsOneWidget),
        );
        expect(results, [false]);
        expect(find.text('Done.'), findsNothing);
      });
    }

    testWidgets('Send back that goes through on Send anyway says it is done', (tester) async {
      final repo = boardRepository();
      var calls = 0;
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenAnswer((_) async {
        if (calls++ == 0) throw refused;
        return receiptOf(const CommandResultTicketChanged(column: ColumnKind.doing, ticketId: 'k1', version: 4));
      });
      final results = await pumpWriter(tester, repo, (context) => ticketWrite(context, ticketSendBack(ticketId: 'k1', feedback: 'x'), retry: false, done: 'Back in Doing.'));
      await tester.tap(sendAnyway);
      await tester.pumpAndSettle();
      expect(results, [true]);
      expect(find.text('Back in Doing.'), findsOneWidget);
    });

    testWidgets("Don't send / Cancel: nothing again, no toast", (tester) async {
      final repo = boardRepository();
      refuseAll(repo);
      final results = await pumpWriter(tester, repo, (context) => ticketWrite(context, ticketSendBack(ticketId: 'k1', feedback: 'x'), retry: false, done: 'Back in Doing.'));
      await tester.tap(find.byKey(const ValueKey('secret-check-cancel')));
      await tester.pumpAndSettle();
      expect(results, [false]);
      expect(find.byType(SnackBar), findsNothing);
      expect(sent(repo), hasLength(1));
    });

    testWidgets('ticket.approve_plan has no confirm_secret: the dialog says what to take out, no Send anyway', (tester) async {
      final repo = boardRepository();
      refuseAll(repo);
      await pumpWriter(tester, repo, (context) => ticketWrite(context, ticketApprovePlan(ticketId: 'k1', decision: PlanApproval.approve, feedback: 'x'), retry: false));
      expect(dialog, findsOneWidget);
      expect(sendAnyway, findsNothing);
      expect(find.text(secretHeldBody(['GitHub token'], secretCheckNoSendAnyway)), findsOneWidget);
    });
  });

  testWidgets('ticket.create (New ticket): dialog, Send anyway with confirm_secret, refused again a toast; the form stays', (tester) async {
    final repo = boardRepository();
    refuseAll(repo);
    await pumpBoardPage(tester, repo, const NewTicketPage(projectId: 'p1'));
    await tester.enterText(find.byKey(const ValueKey('new-ticket-title')), 'Deploy with $fakeSecret');
    await tester.ensureVisible(find.byKey(const ValueKey('new-ticket-create-start')));
    await tester.tap(find.byKey(const ValueKey('new-ticket-create-start')));
    await settleBoard(tester);
    await threeChecks(
      tester,
      repo,
      'ticket.create',
      settle: () => settleBoard(tester),
      noticeShown: () => expect(find.text(secretCheckNoSendAnyway), findsOneWidget),
    );
    expect(find.text('Deploy with $fakeSecret'), findsOneWidget, reason: 'the form keeps what was typed');
  });

  group('the Inbox (approval.answer, approval.decide_plan)', () {
    MockRepository inboxRepository() {
      final repo = boardRepository();
      when(() => repo.linkState).thenReturn(offlineLink);
      when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.approvals()).thenAnswer((_) async => const []);
      when(() => repo.proposals()).thenAnswer((_) async => const []);
      when(() => repo.notifications()).thenAnswer((_) async => const NotificationsSnapshot(notifications: [], unread: 0));
      return repo;
    }

    final approval = approvalOf('a1');

    /// An InboxBloc in the tree and a button that adds [event] with the dialog as its ask.
    Future<InboxBloc> pumpInbox(WidgetTester tester, MockRepository repo, InboxEvent Function(SendAnywayAsk ask) event) async {
      late InboxBloc bloc;
      await tester.pumpWidget(BlocProvider(
        create: (_) => bloc = InboxBloc(repo),
        child: MaterialApp(
          theme: aureliusTheme(),
          home: Scaffold(body: Builder(builder: (context) => TextButton(onPressed: () => context.read<InboxBloc>().add(event(sendAnywayDialog(context))), child: const Text('go')))),
        ),
      ));
      await tester.tap(find.text('go'));
      await tester.pumpAndSettle();
      return bloc;
    }
    for (final (name, event) in [
      ('approval.answer', (SendAnywayAsk ask) => QuestionAnswered(approval, const {'Which token?': 'Use $fakeSecret'}, ask: ask)),
      ('approval.decide_plan', (SendAnywayAsk ask) => PlanDecided(approval, PlanDecision.revise, feedback: 'Use $fakeSecret', ask: ask)),
    ]) {
      testWidgets('$name: dialog, Send anyway with confirm_secret, refused again a notice and the request stays open', (tester) async {
        final repo = inboxRepository();
        refuseAll(repo);
        final bloc = await pumpInbox(tester, repo, event);
        expect(bloc.state.deciding, {'a1'}, reason: 'it waits for the board member while the dialog is open');
        await threeChecks(
          tester,
          repo,
          name,
          settle: () => tester.pumpAndSettle(),
          noticeShown: () {
            expect(bloc.state.notice, secretCheckNoSendAnyway);
            expect(bloc.state.notSent, 'a1', reason: 'the approval page stays to change the text');
            expect(bloc.state.deciding, isEmpty);
          },
        );
        await tester.pumpWidget(const SizedBox());
      });
    }

    testWidgets("Don't send keeps the question open with a notice", (tester) async {
      final repo = inboxRepository();
      refuseAll(repo);
      final bloc = await pumpInbox(tester, repo, (ask) => QuestionAnswered(approval, const {'q': fakeSecret}, ask: ask));
      await tester.tap(find.byKey(const ValueKey('secret-check-cancel')));
      await tester.pumpAndSettle();
      expect(bloc.state.notSent, 'a1');
      expect(bloc.state.notice, 'Not sent. Nothing was answered.');
      expect(sent(repo), hasLength(1));
      await tester.pumpWidget(const SizedBox());
    });
  });
}
