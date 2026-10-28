import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/chat_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

MessageView message(String id, MessageRole role, String text, {int ordinal = 0, bool queued = false, MessageStatus status = MessageStatus.complete}) =>
    messageOf(id, role, text, ordinal: ordinal, queued: queued, status: status);

void main() {
  late MockRepository repo;

  /// The conversation: a running answer and a message waiting behind it.
  final waiting = [
    message('m1', MessageRole.user, 'Build it', ordinal: 1),
    message('a1', MessageRole.assistant, 'Working', ordinal: 2, status: MessageStatus.partial),
    message('m2', MessageRole.user, 'Use Postgres', ordinal: 3, queued: true),
  ];

  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  setUp(() {
    repo = MockRepository();
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.appends).thenAnswer((_) => const Stream.empty());
    when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.online));
    when(() => repo.messages('thread-2')).thenAnswer((_) async => snapshotOf('thread-2', messages: waiting));
    when(() => repo.withdraw(any())).thenReturn(false);
  });

  List<Map<String, dynamic>> commands() => writesOf(repo);

  group('a waiting message', () {
    blocTest<ChatBloc, ChatState>(
      'Delete takes it away at once and sends conversation.delete_queued',
      build: () {
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry')))
            .thenAnswer((_) async => receiptOf(const CommandResultQueuedMessageDeleted(conversationId: 'thread-2', messageId: 'm2', runId: 'r')));
        return ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr');
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        bloc.add(const ChatQueuedDeleted('m2'));
        await Future<void>.delayed(Duration.zero);
        expect(bloc.state.messages.map((m) => m.id), ['m1', 'a1'], reason: 'optimistic');
      },
      wait: const Duration(milliseconds: 150),
      verify: (bloc) {
        expect(commands(), [
          {'method': 'conversation.delete_queued', 'params': {'conversation_id': 'thread-2', 'message_id': 'm2'}},
        ]);
      },
    );

    blocTest<ChatBloc, ChatState>(
      'a refused Delete puts the message back and says why',
      build: () {
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry')))
            .thenThrow(const ProtocolException(ErrorCode.validation, 'the answer to this message has already started; stop it instead'));
        return ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr');
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        bloc.add(const ChatQueuedDeleted('m2'));
      },
      wait: const Duration(milliseconds: 50),
      expect: () => [
        isA<ChatState>().having((s) => s.messages.length, 'messages', 3),
        isA<ChatState>().having((s) => s.messages.map((m) => m.id), 'removed', ['m1', 'a1']),
        isA<ChatState>()
            .having((s) => s.messages.map((m) => m.id), 'back in place', ['m1', 'a1', 'm2'])
            .having((s) => s.notice, 'notice', 'the answer to this message has already started; stop it instead'),
      ],
    );

    blocTest<ChatBloc, ChatState>(
      'Send now sends conversation.send_queued_now',
      build: () {
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry')))
            .thenAnswer((_) async => receiptOf(const CommandResultQueuedMessageSentNow(conversationId: 'thread-2', messageId: 'm2', runId: 'r')));
        return ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr');
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        bloc.add(const ChatQueuedSentNow('m2'));
      },
      wait: const Duration(milliseconds: 100),
      verify: (_) => expect(commands(), [
        {'method': 'conversation.send_queued_now', 'params': {'conversation_id': 'thread-2', 'message_id': 'm2'}},
      ]),
    );

    blocTest<ChatBloc, ChatState>(
      'offline, nothing changes and the notice says so',
      build: () {
        return ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr');
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.offline));
        bloc.add(const ChatQueuedDeleted('m2'));
        await Future<void>.delayed(const Duration(milliseconds: 20));
        bloc.add(const ChatNoticeShown());
        await Future<void>.delayed(const Duration(milliseconds: 20));
        bloc.add(const ChatQueuedSentNow('m2'));
      },
      wait: const Duration(milliseconds: 50),
      skip: 1,
      expect: () => [
        isA<ChatState>().having((s) => s.notice, 'notice', 'Not connected to the PC.').having((s) => s.messages.length, 'kept', 3),
        isA<ChatState>().having((s) => s.notice, 'notice', isNull),
        isA<ChatState>().having((s) => s.notice, 'notice', 'Not connected to the PC.'),
      ],
      verify: (_) => verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry'))),
    );
  });

  group('an unsent message', () {
    blocTest<ChatBloc, ChatState>(
      'discarded while it waits on this phone: taken back, nothing reaches the PC',
      build: () {
        final pending = Completer<Receipt>();
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry'))).thenAnswer((invocation) => pending.future);
        when(() => repo.withdraw(any())).thenAnswer((invocation) {
          pending.completeError(NotSentException(invocation.positionalArguments.single as String));
          return true;
        });
        return ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr');
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        bloc.add(const ChatMessageSent('Never mind'));
        await Future<void>.delayed(const Duration(milliseconds: 20));
        expect(bloc.state.unsent.single.failed, isFalse, reason: 'still sending');
        bloc.add(ChatUnsentDiscarded(bloc.state.unsent.single.localId));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        expect(bloc.state.unsent, isEmpty);
        expect(commands().map((c) => c['method']), ['employee.send_message'], reason: 'no delete afterwards');
        verify(() => repo.withdraw(any())).called(1);
      },
    );

    late Completer<Receipt> onTheWay;

    blocTest<ChatBloc, ChatState>(
      'discarded after it went out: deleted on the PC once its receipt arrives',
      build: () {
        onTheWay = Completer<Receipt>();
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry'))).thenAnswer((invocation) {
          final command = invocation.positionalArguments.single as Map<String, dynamic>;
          if (command['method'] == 'conversation.delete_queued') {
            return Future.value(receiptOf(const CommandResultQueuedMessageDeleted(conversationId: 'thread-2', messageId: 'm9', runId: 'r9')));
          }
          return onTheWay.future;
        });
        return ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr');
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        bloc.add(const ChatMessageSent('Never mind'));
        await Future<void>.delayed(const Duration(milliseconds: 20));
        bloc.add(ChatUnsentDiscarded(bloc.state.unsent.single.localId));
        await Future<void>.delayed(const Duration(milliseconds: 20));
        expect(bloc.state.unsent, isEmpty, reason: 'gone from the screen at once');
        onTheWay.complete(receiptOf(const CommandResultEmployeeMessageSent(conversationId: 'thread-2', messageId: 'm9', runId: 'r9')));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        expect(bloc.state.unsent, isEmpty);
        final sent = commands();
        expect(sent.map((c) => c['method']), ['employee.send_message', 'conversation.delete_queued']);
        expect(sent.last, {'method': 'conversation.delete_queued', 'params': {'conversation_id': 'thread-2', 'message_id': 'm9'}});
      },
    );
  });
}
