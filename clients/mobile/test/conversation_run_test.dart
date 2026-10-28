import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/chat_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

/// The model chip of an employee chat sets that conversation only
/// (`conversation.set_run`), never the employee's or the global default.
void main() {
  late MockRepository repo;

  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  setUp(() {
    repo = MockRepository();
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.appends).thenAnswer((_) => const Stream.empty());
    when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.online));
    when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
        .thenAnswer((_) async => receiptOf(const CommandResultConversationRunSet(conversationId: 'c1')));
  });

  blocTest<ChatBloc, ChatState>(
    'the snapshot carries the conversation\'s own model and effort',
    build: () {
      when(() => repo.messages('c1')).thenAnswer((_) async => snapshotOf('c1', run: const TaskRun(model: 'sonnet', effort: 'high')));
      return ChatBloc(repo, conversationId: 'c1', employeeId: 'mgr');
    },
    wait: const Duration(milliseconds: 150),
    verify: (bloc) {
      expect((bloc.state.runModel, bloc.state.runEffort), ('sonnet', 'high'));
      expect(bloc.state.overridden, isTrue);
    },
  );

  blocTest<ChatBloc, ChatState>(
    'choosing a model sends conversation.set_run, and the default clears it',
    build: () {
      when(() => repo.messages('c1')).thenAnswer((_) async => snapshotOf('c1'));
      return ChatBloc(repo, conversationId: 'c1', employeeId: 'mgr');
    },
    act: (bloc) async {
      await Future<void>.delayed(const Duration(milliseconds: 150));
      bloc.add(const ChatRunSelected(model: 'sonnet'));
      await Future<void>.delayed(Duration.zero);
      expect(bloc.state.runModel, 'sonnet', reason: 'optimistic');
      bloc.add(const ChatRunSelected());
    },
    wait: const Duration(milliseconds: 50),
    verify: (bloc) {
      expect(writesOf(repo), [
        {'method': 'conversation.set_run', 'params': {'conversation_id': 'c1', 'model': 'sonnet'}},
        {'method': 'conversation.set_run', 'params': {'conversation_id': 'c1'}},
      ]);
      expect(bloc.state.overridden, isFalse);
    },
  );
}
