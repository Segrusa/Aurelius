import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/commands.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/chat/chat_bloc.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/fake_daemon.dart';
import 'support/tickets.dart';

// B10 on the phone (AUR-MU-04-M1, E6): a board member's message that holds a
// vault secret's value is refused with `looks_like_secret` and the names of
// the items; the phone asks, and "Send anyway" sends it again with
// `confirm_secret`. Neutral names (Alex, Robin) and a fake secret only.

const fakeSecret = 'sk-test-FAKE-0000';

const refused = ProtocolException(ErrorCode.looksLikeSecret, 'This looks like a vault secret.', matchedItems: ['GitHub token']);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  /// The writes of [repo] with their idempotency keys, in order.
  List<(Map<String, dynamic>, String?)> writesWithKeys(MockRepository repo) {
    final captured = verify(() => repo.write(captureAny(), idempotencyKey: captureAny(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).captured;
    return [for (var i = 0; i < captured.length; i += 2) (captured[i] as Map<String, dynamic>, captured[i + 1] as String?)];
  }

  /// Refuses the first write as holding a secret, then takes it with [result].
  void refuseFirst(MockRepository repo, CommandResult result) {
    var calls = 0;
    when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenAnswer((_) async {
      if (calls++ == 0) throw refused;
      return receiptOf(result);
    });
  }

  MockRepository chatRepository() {
    final repo = boardRepository();
    when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.appends).thenAnswer((_) => const Stream.empty());
    when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.boardMembers()).thenAnswer((_) async => const []);
    when(() => repo.messages(any())).thenAnswer((_) async => snapshotOf('c1'));
    return repo;
  }

  group('ChatBloc', () {
    test('Ticket discussion: refused with only the names, nothing sent twice silently; Send anyway goes with confirm_secret and a new key', () async {
      final repo = chatRepository();
      refuseFirst(repo, const CommandResultGroupMessageSent(conversationId: 'team', messageId: 'm1', runIds: [], to: []));
      final bloc = ChatBloc.discussion(repo, ticketId: 'k1', conversationId: 'c1');
      await stateWhere(bloc, (s) => !s.loading);
      bloc.add(const ChatMessageSent('The token is $fakeSecret'));
      final asked = await stateWhere(bloc, (s) => s.secretCheck != null);
      final unsent = asked.unsent.single;
      expect(unsent.secretItems, ['GitHub token']);
      expect(unsent.failed, isTrue);
      expect(asked.notice, isNull, reason: 'the question replaces the error toast');
      bloc.add(const ChatSecretCheckShown());
      bloc.add(ChatSecretConfirmed(unsent.localId));
      await stateWhere(bloc, (s) => s.unsent.isEmpty || s.unsent.single.delivered);
      final writes = writesWithKeys(repo);
      expect(writes, hasLength(2));
      expect(writes[0].$1['method'], 'ticket.post');
      expect(paramsOf(writes[0].$1).containsKey('confirm_secret'), isFalse);
      expect(paramsOf(writes[1].$1)['confirm_secret'], isTrue);
      expect(writes[1].$2, isNot(writes[0].$2), reason: 'the confirmed send is another request');
      await bloc.close();
    });

    test('M-m5: refused again after Send anyway (a value in an attached file) ends unsent with a notice, no second question', () async {
      final repo = chatRepository();
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenThrow(refused);
      final bloc = ChatBloc.discussion(repo, ticketId: 'k1', conversationId: 'c1');
      await stateWhere(bloc, (s) => !s.loading);
      bloc.add(const ChatMessageSent('See the attached notes'));
      final asked = await stateWhere(bloc, (s) => s.secretCheck != null);
      bloc.add(const ChatSecretCheckShown());
      await stateWhere(bloc, (s) => s.secretCheck == null);
      bloc.add(ChatSecretConfirmed(asked.unsent.single.localId));
      final ended = await stateWhere(bloc, (s) => s.notice != null);
      expect(ended.notice, 'Take the value out of the message or file.');
      expect(ended.secretCheck, isNull, reason: 'no new question, so no loop');
      expect(ended.unsent.single.failed, isTrue);
      expect(ended.unsent.single.secretItems, isEmpty);
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(writesWithKeys(repo), hasLength(2), reason: 'one refused send and one refused Send anyway, nothing more');
      await bloc.close();
    });

    // A project chat takes confirm_secret since AUR-MU-04-F5: see send_anyway_test.dart (M4).

    test("an employee's chat: Send anyway sends employee.send_message with confirm_secret", () async {
      final repo = chatRepository();
      refuseFirst(repo, const CommandResultEmployeeMessageSent(conversationId: 'c1', messageId: 'm1', runId: 'r1'));
      final bloc = ChatBloc(repo, conversationId: 'c1', employeeId: 'e-forge');
      await stateWhere(bloc, (s) => !s.loading);
      bloc.add(const ChatMessageSent('Use $fakeSecret'));
      final asked = await stateWhere(bloc, (s) => s.secretCheck != null);
      bloc.add(ChatSecretConfirmed(asked.unsent.single.localId));
      await stateWhere(bloc, (s) => s.unsent.isEmpty || s.unsent.single.delivered);
      final writes = writesWithKeys(repo);
      expect(writes.map((w) => w.$1['method']), ['employee.send_message', 'employee.send_message']);
      expect(paramsOf(writes[1].$1)['confirm_secret'], isTrue);
      await bloc.close();
    });

    test('Try again on an ordinary failure keeps confirm_secret off; the commands leave it out unless confirmed', () {
      expect(paramsOf(Commands.postTicketMessage('k1', 'x')).containsKey('confirm_secret'), isFalse);
      expect(paramsOf(Commands.postTicketMessage('k1', 'x', confirmSecret: true))['confirm_secret'], isTrue);
      expect(paramsOf(Commands.postTicketMessage('k1', 'x', confirmSecret: true))['confirm_secret'], isTrue);
      expect(paramsOf(Commands.sendEmployeeMessage('e', 'x', conversationId: 'c', confirmSecret: true))['confirm_secret'], isTrue);
    });
  });

  test('the connection carries matched_items of looks_like_secret into the exception', () async {
    final daemon = FakeDaemon()..addUser('u-1', 'alex', 'alex long password');
    final store = await storeWithSecret();
    final connection = AureliusConnection(host: testHost, store: store, api: FakeApi(), connect: daemon.connect, deviceName: 'Test phone');
    addTearDown(connection.dispose);
    await store.setSessionToken(testHost.hostId, daemon.sessionFor('u-1'));
    connection.start();
    await until(() => connection.state.isOnline && connection.state.user != null);
    daemon.errors['ticket.post'] = {'code': 'looks_like_secret', 'message': 'This looks like a vault secret.', 'matched_items': ['GitHub token', 'Sentry DSN']};
    await expectLater(
      connection.write(Commands.postTicketMessage('k1', 'Hi Robin, $fakeSecret'), retry: false),
      throwsA(isA<ProtocolException>().having((e) => e.looksLikeSecret, 'looksLikeSecret', isTrue).having((e) => e.matchedItems, 'matchedItems', ['GitHub token', 'Sentry DSN'])),
    );
  });

  testWidgets("a ticket's discussion: the dialog names the items, never the value; Don't send keeps it; Send anyway posts it with confirm_secret (P-46)", (tester) async {
    final repo = chatRepository();
    final ticket = TicketView.fromJson({
      ...ticketOf(column: ColumnKind.doing, ownerEmployeeId: 'forge', ownerName: 'Forge', rootTaskId: 'root').toJson(),
      'conversation_id': 'disc-1',
    });
    when(() => repo.ticket(any())).thenAnswer((_) async => TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []));
    when(() => repo.messages(any())).thenAnswer((_) async => snapshotOf('disc-1'));
    when(() => repo.load()).thenAnswer((_) async => loadOf());
    refuseFirst(repo, const CommandResultTicketMessagePosted(conversationId: 'disc-1', messageId: 'm1', ticketId: 'k1', runIds: [], to: []));
    const link = LinkState(
      status: LinkStatus.online,
      capabilities: ['read', 'chat', 'task_control', 'approve'],
      features: ['ticket_discussion', 'vault'],
      boardMembersExist: true,
      user: testUser,
    );
    await pumpBoardPage(tester, repo, const TicketPage(ticketId: 'k1'), link: link);
    await tester.enterText(find.byType(TextField).last, 'Deploy with $fakeSecret');
    await tester.pump();
    await tester.tap(find.byKey(const Key('composer-send')));
    await settleBoard(tester);
    expect(find.byKey(const ValueKey('secret-check')), findsOneWidget);
    expect(find.text('This looks like a vault secret. Send anyway?'), findsOneWidget);
    expect(find.descendant(of: find.byKey(const ValueKey('secret-check')), matching: find.text('GitHub token')), findsOneWidget);
    expect(find.descendant(of: find.byKey(const ValueKey('secret-check')), matching: find.textContaining(fakeSecret)), findsNothing);
    await tester.tap(find.byKey(const ValueKey('secret-check-cancel')));
    await settleBoard(tester);
    expect(find.text('Not sent · looks like a vault secret'), findsOneWidget);
    await tester.tap(find.byKey(const Key('unsent-send-anyway')));
    await settleBoard(tester);
    await tester.tap(find.byKey(const ValueKey('secret-check-send')));
    await settleBoard(tester);
    final posts = writesOf(repo);
    expect(posts.map((p) => p['method']), ['ticket.post', 'ticket.post']);
    expect(paramsOf(posts.last)['confirm_secret'], isTrue);
    expect(find.text('Not sent · looks like a vault secret'), findsNothing);
  });
}
