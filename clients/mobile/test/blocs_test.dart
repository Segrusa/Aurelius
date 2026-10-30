import 'dart:async';
import 'dart:io';

import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/gateway_api.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/chat/chat_bloc.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_sections.dart';
import 'package:aurelius_mobile/features/pairing/pairing_bloc.dart';
import 'package:aurelius_mobile/features/updates/updates_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';

class MockRepository extends Mock implements AureliusRepository {}

class MockApi extends Mock implements GatewayApi {}

MessageView message(String id, MessageRole role, String text, {MessageStatus status = MessageStatus.complete, String? author, String? update}) => messageOf(id, role, text, status: status, author: author, update: update);

ApprovalView approval(String id) => approvalOf(id);

Receipt receipt(CommandResult result) => receiptOf(result);

CommandResultEmployeeMessageSent managerSent(String conversationId) => CommandResultEmployeeMessageSent(conversationId: conversationId, messageId: 'm', runId: 'r');

Map<String, dynamic> params(Map<String, dynamic> request) => request['params'] as Map<String, dynamic>;

void main() {
  late MockRepository repo;
  late StreamController<StreamAppend> appends;

  setUpAll(() {
    registerFallbackValue(<String, dynamic>{});
    registerFallbackValue(File(''));
  });

  setUp(() {
    repo = MockRepository();
    appends = StreamController<StreamAppend>.broadcast();
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.companyChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.changes).thenAnswer((_) => const Stream.empty());
    when(() => repo.appends).thenAnswer((_) => appends.stream);
    when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.online));
  });

  tearDown(() => appends.close());

  group('ChatBloc in the group chat (Team)', () {
    MessagesSnapshot team(List<MessageView> messages) => snapshotOf('team', messages: messages);

    test('a server from before the Team chat (unknown_method) stops the loading and says to restart it; Try again loads it', () async {
      var calls = 0;
      when(() => repo.groupChat()).thenAnswer((_) async {
        if (calls++ == 0) throw const ProtocolException(ErrorCode.unknownMethod, 'This server does not know the method group_chat.get.');
        return team([message('u1', MessageRole.user, 'Hi all')]);
      });
      final bloc = ChatBloc.group(repo);
      await Future<void>.delayed(const Duration(milliseconds: 150));
      expect(bloc.state.loading, isFalse);
      expect(bloc.state.loadError, contains('older than this app'));
      expect(bloc.state.loadError, contains('Restart the server'));
      bloc.add(const ChatRefreshed());
      await Future<void>.delayed(const Duration(milliseconds: 150));
      expect((bloc.state.loading, bloc.state.loadError, bloc.state.conversationId), (false, null, 'team'));
      await bloc.close();
    });

    test('a server that never answers, or an answer the app cannot read, stops the loading with the reason', () async {
      var calls = 0;
      when(() => repo.groupChat()).thenAnswer((_) async {
        if (calls++ == 0) throw const DisconnectedException('The PC did not answer in time.');
        throw StateError('bad answer');
      });
      final bloc = ChatBloc.group(repo);
      await Future<void>.delayed(const Duration(milliseconds: 150));
      expect((bloc.state.loading, bloc.state.loadError), (false, 'The PC did not answer in time.'));
      bloc.add(const ChatRefreshed());
      await Future<void>.delayed(const Duration(milliseconds: 150));
      expect(bloc.state.loading, isFalse);
      expect(bloc.state.loadError, contains('could not be loaded'));
      await bloc.close();
    });

    blocTest<ChatBloc, ChatState>(
      'loads through group_chat.get and learns its id; each message keeps its author',
      build: () {
        when(() => repo.groupChat()).thenAnswer((_) async => team([
              message('u1', MessageRole.user, 'Who reviews this?'),
              message('a1', MessageRole.assistant, 'I will.', author: 'Trace', update: 'group'),
              message('a2', MessageRole.assistant, 'And I test it.', author: 'Pulse', update: 'group'),
            ]));
        return ChatBloc.group(repo);
      },
      wait: const Duration(milliseconds: 150),
      verify: (bloc) {
        expect(bloc.state.loading, isFalse);
        expect(bloc.state.conversationId, 'team');
        expect(bloc.state.messages.map((m) => m.author), [null, 'Trace', 'Pulse']);
        verify(() => repo.groupChat()).called(1);
        verifyNever(() => repo.messages(any()));
      },
    );

    blocTest<ChatBloc, ChatState>(
      'a message goes out with group_chat.send (text, images, files), never interrupting',
      build: () {
        when(() => repo.groupChat()).thenAnswer((_) async => team([message('a1', MessageRole.assistant, 'Working', status: MessageStatus.partial, author: 'Atlas')]));
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
            .thenAnswer((_) async => receipt(const CommandResultGroupMessageSent(conversationId: 'team', messageId: 'u2', runIds: ['r2'], to: ['Orion'])));
        return ChatBloc.group(repo);
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        bloc.add(const ChatMessageSent('@Orion plan it', now: true));
      },
      wait: const Duration(milliseconds: 200),
      verify: (bloc) {
        final sent = writesOf(repo);
        expect(sent.map((c) => c['method']), ['group_chat.send'], reason: 'no run.stop first');
        expect(params(sent.single), {'text': '@Orion plan it', 'images': <String>[], 'files': <String>[]});
        expect(bloc.state.unsent, isEmpty);
      },
    );

    blocTest<ChatBloc, ChatState>(
      'an answer streams in under its employee before it is a message, then the snapshot takes over',
      build: () {
        var calls = 0;
        when(() => repo.groupChat()).thenAnswer((_) async => team([
              message('u1', MessageRole.user, '@Orion status?'),
              if (calls++ > 0) message('live', MessageRole.assistant, 'All on track.', author: 'Orion', update: 'group'),
            ]));
        return ChatBloc.group(repo, runAuthor: (run) => run == 'r-orion' ? 'Orion' : null);
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        appends.add(const StreamAppend(runId: 'r-orion', conversationId: 'team', messageId: 'live', offset: 0, text: 'All on'));
        await Future<void>.delayed(const Duration(milliseconds: 20));
        appends.add(const StreamAppend(runId: 'r-orion', conversationId: 'team', messageId: 'live', offset: 6, text: ' track'));
        await Future<void>.delayed(const Duration(milliseconds: 20));
        final live = bloc.state.messages.last;
        expect((live.id, live.text, live.author, live.isStreaming), ('live', 'All on track', 'Orion', true));
        expect(bloc.state.streaming?.id, 'live');
        verify(() => repo.groupChat()).called(1);
        bloc.add(const ChatRefreshed());
      },
      wait: const Duration(milliseconds: 200),
      verify: (bloc) {
        expect(bloc.state.messages.map((m) => (m.id, m.text, m.author)), [('u1', '@Orion status?', null), ('live', 'All on track.', 'Orion')]);
        expect(bloc.state.streaming, isNull);
      },
    );

    blocTest<ChatBloc, ChatState>(
      'a live answer stays through a refresh while its run works',
      build: () {
        when(() => repo.groupChat()).thenAnswer((_) async => team([message('u1', MessageRole.user, 'Hi all')]));
        return ChatBloc.group(repo, runAuthor: (run) => 'Atlas');
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        appends.add(const StreamAppend(runId: 'r', conversationId: 'team', messageId: 'live', offset: 0, text: 'Hello'));
        await Future<void>.delayed(const Duration(milliseconds: 20));
        bloc.add(const ChatRefreshed());
      },
      wait: const Duration(milliseconds: 200),
      verify: (bloc) => expect(bloc.state.messages.map((m) => (m.id, m.author)), [('u1', null), ('live', 'Atlas')]),
    );

    blocTest<ChatBloc, ChatState>(
      'a live answer whose run ended without a message goes by itself, with no refresh',
      build: () {
        when(() => repo.groupChat()).thenAnswer((_) async => team([message('u1', MessageRole.user, 'Hi all')]));
        var running = true;
        final bloc = ChatBloc.group(repo, runAuthor: (run) => running ? 'Atlas' : null, liveTimeout: const Duration(milliseconds: 100));
        // The run ends without its answer being copied (it only posted with post_to_group_chat).
        Timer(const Duration(milliseconds: 250), () => running = false);
        return bloc;
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        appends.add(const StreamAppend(runId: 'r', conversationId: 'team', messageId: 'live', offset: 0, text: ''));
        await Future<void>.delayed(const Duration(milliseconds: 80));
        expect(bloc.state.streaming?.id, 'live', reason: 'its run still works');
      },
      wait: const Duration(milliseconds: 700),
      verify: (bloc) {
        expect(bloc.state.messages.map((m) => m.id), ['u1']);
        expect(bloc.state.streaming, isNull);
        verify(() => repo.groupChat()).called(1);
      },
    );

    blocTest<ChatBloc, ChatState>(
      'a live answer gets its author once the company knows the run, without a next delta',
      build: () {
        when(() => repo.groupChat()).thenAnswer((_) async => team([message('u1', MessageRole.user, 'Hi all')]));
        String? author;
        Timer(const Duration(milliseconds: 220), () => author = 'Orion');
        return ChatBloc.group(repo, runAuthor: (run) => author, liveTimeout: const Duration(seconds: 5));
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        appends.add(const StreamAppend(runId: 'r', conversationId: 'team', messageId: 'live', offset: 0, text: 'Looking'));
        await Future<void>.delayed(const Duration(milliseconds: 20));
        expect(bloc.state.messages.last.author, isNull);
      },
      wait: const Duration(milliseconds: 1300),
      verify: (bloc) => expect(bloc.state.messages.map((m) => (m.id, m.author)), [('u1', null), ('live', 'Orion')]),
    );
  });

  group('ChatBloc', () {
    blocTest<ChatBloc, ChatState>(
      'a first message creates the project conversation and loads it',
      build: () {
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
            .thenAnswer((_) async => receipt(const CommandResultProjectMessageSent(conversationId: 'conv-1', messageId: 'm1', projectId: 'p', runId: 'run')));
        when(() => repo.messages('conv-1')).thenAnswer((_) async => snapshotOf('conv-1', messages: [message('m1', MessageRole.user, 'Hello'), message('a1', MessageRole.assistant, '', status: MessageStatus.partial)]));
        return ChatBloc(repo, projectId: 'p');
      },
      act: (bloc) => bloc.add(const ChatMessageSent('Hello')),
      wait: const Duration(milliseconds: 200),
      verify: (bloc) {
        expect(bloc.state.conversationId, 'conv-1');
        expect(bloc.state.unsent, isEmpty);
        expect(bloc.state.messages, hasLength(2));
        expect(bloc.state.streaming?.id, 'a1');
      },
    );

    blocTest<ChatBloc, ChatState>(
      'streamed appends extend the answer; a gap refetches',
      build: () {
        when(() => repo.messages('conv-1')).thenAnswer((_) async => snapshotOf('conv-1', messages: [message('a1', MessageRole.assistant, 'Hi', status: MessageStatus.partial)]));
        return ChatBloc(repo, conversationId: 'conv-1', projectId: 'p');
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        appends.add(const StreamAppend(runId: 'run', conversationId: 'conv-1', messageId: 'a1', offset: 2, text: ' there'));
        appends.add(const StreamAppend(runId: 'run', conversationId: 'other', messageId: 'x', offset: 0, text: 'ignored'));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) => expect(bloc.state.messages.single.text, 'Hi there'),
    );

    blocTest<ChatBloc, ChatState>(
      'an earlier employee chat the server marks read-only loads as history',
      build: () {
        when(() => repo.messages('old-1')).thenAnswer((_) async => snapshotOf('old-1', messages: [message('m1', MessageRole.user, 'Hello')], readOnly: true));
        return ChatBloc(repo, conversationId: 'old-1', employeeId: 'mgr');
      },
      wait: const Duration(milliseconds: 150),
      verify: (bloc) => expect(bloc.state.history, isTrue),
    );

    blocTest<ChatBloc, ChatState>(
      'a reply too large to send is kept as a visible load error, cleared by the next load',
      build: () {
        var calls = 0;
        when(() => repo.messages('conv-1')).thenAnswer((_) async {
          if (calls++ == 0) throw const ProtocolException(ErrorCode.frameTooLarge, "The server's reply is too large to send (4.8 MB).");
          return snapshotOf('conv-1', messages: [message('a1', MessageRole.assistant, 'Hi')]);
        });
        return ChatBloc(repo, conversationId: 'conv-1', projectId: 'p');
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        expect(bloc.state.loading, isFalse);
        expect(bloc.state.loadError, "The server's reply is too large to send (4.8 MB).");
        bloc.add(const ChatRefreshed());
      },
      wait: const Duration(milliseconds: 150),
      verify: (bloc) {
        expect(bloc.state.loadError, isNull);
        expect(bloc.state.messages, hasLength(1));
        verify(() => repo.messages('conv-1')).called(2);
      },
    );

    blocTest<ChatBloc, ChatState>(
      'an undelivered message stays as "Not sent" and retries with the same idempotency key',
      build: () {
        var calls = 0;
        final ids = <String?>[];
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
            .thenAnswer((invocation) async {
          ids.add(invocation.namedArguments[#idempotencyKey] as String?);
          if (calls++ == 0) throw NotSentException(ids.last!);
          expect(ids.last, ids.first, reason: 'same idempotency key');
          return receipt(const CommandResultProjectMessageSent(conversationId: 'conv-1', messageId: 'm1', projectId: 'p', runId: 'run'));
        });
        when(() => repo.messages('conv-1')).thenAnswer((_) async => snapshotOf('conv-1', messages: []));
        return ChatBloc(repo, projectId: 'p');
      },
      act: (bloc) async {
        bloc.add(const ChatMessageSent('Hello'));
        await Future<void>.delayed(const Duration(milliseconds: 50));
        expect(bloc.state.unsent.single.failed, isTrue);
        bloc.add(ChatUnsentRetried(bloc.state.unsent.single.localId));
      },
      wait: const Duration(milliseconds: 150),
      verify: (bloc) => expect(bloc.state.unsent, isEmpty),
    );

    blocTest<ChatBloc, ChatState>(
      'an employee message goes to the open conversation, not always the main one',
      build: () {
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
            .thenAnswer((_) async => receipt(managerSent('thread-2')));
        when(() => repo.messages('thread-2')).thenAnswer((_) async => snapshotOf('thread-2'));
        return ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr');
      },
      act: (bloc) => bloc.add(const ChatMessageSent('Plan the launch')),
      wait: const Duration(milliseconds: 150),
      verify: (_) {
        final command = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry'))).captured.single as Map<String, dynamic>;
        expect(command['method'], 'employee.send_message');
        expect(params(command), {'employee_id': 'mgr', 'text': 'Plan the launch', 'conversation_id': 'thread-2', 'images': [], 'files': []});
      },
    );

    blocTest<ChatBloc, ChatState>(
      'an image is uploaded, waits in the composer and goes with the next message (text optional)',
      build: () {
        when(() => repo.upload(any(), any())).thenAnswer((_) async => const UploadedFile(path: r'C:\inbox\shot.png', name: 'shot.png', byteLength: 10));
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
            .thenAnswer((_) async => receipt(managerSent('thread-2')));
        when(() => repo.messages('thread-2')).thenAnswer((_) async => snapshotOf('thread-2'));
        return ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr');
      },
      act: (bloc) async {
        bloc.add(ChatImageAdded(File('shot.png'), 'shot.png'));
        await Future<void>.delayed(const Duration(milliseconds: 50));
        expect(bloc.state.images.single.path, r'C:\inbox\shot.png');
        bloc.add(const ChatMessageSent(''));
      },
      wait: const Duration(milliseconds: 150),
      verify: (bloc) {
        expect(bloc.state.images, isEmpty, reason: 'sent with the message');
        final command = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry'))).captured.single as Map<String, dynamic>;
        expect(command['method'], 'employee.send_message');
        expect(params(command), {'employee_id': 'mgr', 'text': '', 'conversation_id': 'thread-2', 'images': [r'C:\inbox\shot.png'], 'files': []});
      },
    );

    group('files for an employee', () {
      late File doc;

      setUp(() async {
        doc = File('${(await Directory.systemTemp.createTemp('aurelius-test')).path}/spec.pdf')..writeAsBytesSync(List.filled(2048, 1));
        when(() => repo.upload(any(), any())).thenAnswer((_) async => const UploadedFile(path: r'C:\inbox\spec.pdf', name: 'spec.pdf', byteLength: 2048));
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
            .thenAnswer((_) async => receipt(managerSent('thread-2')));
        when(() => repo.messages('thread-2')).thenAnswer((_) async => snapshotOf('thread-2'));
      });

      blocTest<ChatBloc, ChatState>(
        'a file is uploaded, waits as a chip and goes with the next message as files, never attachment.add',
        build: () => ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr'),
        act: (bloc) async {
          bloc.add(ChatFileAttached(doc, 'spec.pdf'));
          await Future<void>.delayed(const Duration(milliseconds: 50));
          expect(bloc.state.files.single, const PendingFile(path: r'C:\inbox\spec.pdf', name: 'spec.pdf', byteLength: 2048));
          bloc.add(const ChatMessageSent(''));
        },
        wait: const Duration(milliseconds: 150),
        verify: (bloc) {
          expect(bloc.state.files, isEmpty, reason: 'sent with the message');
          final command = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry'))).captured.single as Map<String, dynamic>;
          expect(command['method'], 'employee.send_message');
          expect(params(command), {'employee_id': 'mgr', 'text': '', 'conversation_id': 'thread-2', 'images': [], 'files': [r'C:\inbox\spec.pdf']});
        },
      );

      blocTest<ChatBloc, ChatState>(
        'a removed file is not sent',
        build: () => ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr'),
        act: (bloc) async {
          bloc.add(ChatFileAttached(doc, 'spec.pdf'));
          await Future<void>.delayed(const Duration(milliseconds: 50));
          bloc.add(ChatFileRemoved(bloc.state.files.single));
          bloc.add(const ChatMessageSent('Hi'));
        },
        wait: const Duration(milliseconds: 150),
        verify: (_) {
          final command = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry'))).captured.single as Map<String, dynamic>;
          expect(params(command)['files'], isEmpty);
        },
      );

      blocTest<ChatBloc, ChatState>(
        'several files picked at once each become a chip; one over 25 MB is refused by name and the rest goes on',
        build: () {
          when(() => repo.upload(any(), any())).thenAnswer((call) async {
            final name = call.positionalArguments[1] as String;
            return UploadedFile(path: 'C:\\inbox\\$name', name: name, byteLength: 2048);
          });
          return ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr');
        },
        act: (bloc) async {
          final dir = doc.parent.path;
          final big = File('$dir/big.mov');
          big.openSync(mode: FileMode.write)
            ..truncateSync(ChatBloc.maxFileBytes + 1)
            ..closeSync();
          final notes = File('$dir/notes.txt')..writeAsStringSync('hi');
          final notices = <String>[];
          final sub = bloc.stream.listen((s) {
            if (s.notice != null) notices.add(s.notice!);
          });
          bloc.add(ChatPicked([(doc, 'spec.pdf'), (big, 'big.mov'), (notes, 'notes.txt')]));
          await Future<void>.delayed(const Duration(milliseconds: 100));
          await sub.cancel();
          expect(notices, ['big.mov is larger than 25 MB and was not added.']);
        },
        verify: (bloc) {
          expect(bloc.state.files.map((f) => f.name), ['spec.pdf', 'notes.txt']);
          verifyNever(() => repo.upload(any(), 'big.mov'));
        },
      );
    });

    blocTest<ChatBloc, ChatState>(
      'an empty message without images sends nothing',
      build: () => ChatBloc(repo, projectId: 'p'),
      act: (bloc) => bloc.add(const ChatMessageSent('  ')),
      wait: const Duration(milliseconds: 50),
      verify: (_) => verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry'))),
    );

    blocTest<ChatBloc, ChatState>(
      '"Send now" stops the running answer before sending; plain send waits for it',
      build: () {
        when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
            .thenAnswer((_) async => receipt(managerSent('thread-2')));
        when(() => repo.messages('thread-2')).thenAnswer((_) async => snapshotOf('thread-2', messages: [message('m1', MessageRole.user, 'Build it'), message('a1', MessageRole.assistant, 'Working', status: MessageStatus.partial)]));
        return ChatBloc(repo, conversationId: 'thread-2', employeeId: 'mgr');
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        bloc.add(const ChatMessageSent('Use Postgres instead', now: true));
        await Future<void>.delayed(const Duration(milliseconds: 100));
        bloc.add(const ChatMessageSent('And add tests'));
      },
      wait: const Duration(milliseconds: 150),
      verify: (_) {
        final commands = verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), retry: any(named: 'retry'))).captured.cast<Map<String, dynamic>>();
        expect(commands.map((c) => c['method']), ['run.stop', 'employee.send_message', 'employee.send_message']);
        expect(params(commands.first)['run_id'], 'run');
        // The PC also stops it with the message itself; a plain send doesn't ask for that.
        expect(params(commands[1])['interrupt'], true);
        expect(params(commands[2]).containsKey('interrupt'), false);
      },
    );

    blocTest<ChatBloc, ChatState>(
      'streamed tool activity shows while the answer is written',
      build: () {
        when(() => repo.messages('conv-1')).thenAnswer((_) async => snapshotOf('conv-1', messages: [message('a1', MessageRole.assistant, 'Hi', status: MessageStatus.partial)]));
        return ChatBloc(repo, conversationId: 'conv-1', projectId: 'p');
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 150));
        appends.add(const StreamAppend(runId: 'run', conversationId: 'conv-1', messageId: 'a1', offset: 2, text: '. Running', activity: [
          ActivityView(callId: 't1', tool: 'Bash', input: {'command': 'ls'}, offset: 2),
        ]));
        appends.add(const StreamAppend(runId: 'run', conversationId: 'conv-1', messageId: 'a1', offset: 11, text: ' tests'));
      },
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        final answer = bloc.state.messages.single;
        expect(answer.text, 'Hi. Running tests');
        expect(answer.activity.single.running, isTrue, reason: 'an append without activity keeps what came before');
        expect(answer.parts, ['Hi', answer.activity.single, '. Running tests']);
      },
    );
  });

  group('InboxBloc', () {
    NotificationView note(String id, {bool read = false, String severity = 'info', String targetType = 'task', String? taskId}) =>
        NotificationView(id: id, severity: severity, title: 't$id', subject: '', createdAtMs: 1, read: read, targetType: targetType, taskId: taskId);
    Future<InboxState> loaded(InboxBloc bloc) => stateWhere(bloc, (s) => !s.loading);

    blocTest<InboxBloc, InboxState>(
      'clear all empties the results at once and keeps approvals',
      build: () {
        when(() => repo.approvals()).thenAnswer((_) async => [approval('a1')]);
        when(() => repo.proposals()).thenAnswer((_) async => []);
        when(() => repo.notifications()).thenAnswer((_) async => NotificationsSnapshot(notifications: [note('n1'), note('n2', read: true)], unread: 1));
        when(() => repo.dismissAllNotifications()).thenAnswer((_) async => 2);
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await loaded(bloc);
        bloc.add(const NotificationsCleared());
        await untilCalled(() => repo.dismissAllNotifications());
      },
      verify: (bloc) {
        expect(bloc.state.notifications, isEmpty);
        expect(bloc.state.unread, 0);
        expect(bloc.state.approvals, hasLength(1));
        verify(() => repo.dismissAllNotifications()).called(1);
        verifyNever(() => repo.dismissNotification(any()));
      },
    );

    blocTest<InboxBloc, InboxState>(
      'opening a task reads its results: they leave the inbox and the badge, an alert to act on stays',
      build: () {
        when(() => repo.approvals()).thenAnswer((_) async => []);
        when(() => repo.proposals()).thenAnswer((_) async => []);
        when(() => repo.notifications()).thenAnswer((_) async => NotificationsSnapshot(
              notifications: [
                note('done', targetType: 'task.succeeded', taskId: 't1'),
                note('fail', severity: 'warning', targetType: 'task.failed', taskId: 't1'),
                note('ask', severity: 'action', targetType: 'task.escalated', taskId: 't1'),
                note('other', targetType: 'task.succeeded', taskId: 't2'),
              ],
              unread: 4,
            ));
        when(() => repo.dismissNotification(any())).thenAnswer((_) async {});
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await loaded(bloc);
        expect(bloc.state.badge, 2, reason: 'the failure and the alert; results give none');
        bloc.add(const TaskOpened('t1'));
        await untilCalled(() => repo.dismissNotification('fail'));
      },
      verify: (bloc) {
        expect(bloc.state.notifications.map((n) => n.id), ['ask', 'other']);
        expect(bloc.state.unread, 2);
        expect(bloc.state.badge, 1);
        verify(() => repo.dismissNotification('done')).called(1);
        verify(() => repo.dismissNotification('fail')).called(1);
        verifyNever(() => repo.dismissNotification('ask'));
        verifyNever(() => repo.dismissNotification('other'));
      },
    );

    blocTest<InboxBloc, InboxState>(
      'a colleague an employee proposed waits in the inbox until you decide',
      build: () {
        var pending = [proposalOf('pr1')];
        when(() => repo.approvals()).thenAnswer((_) async => []);
        when(() => repo.notifications()).thenAnswer((_) async => const NotificationsSnapshot(notifications: [], unread: 0));
        when(() => repo.proposals()).thenAnswer((_) async => pending);
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async {
          pending = [];
          return receipt(const CommandResultProposalDecided(proposalId: 'pr1', status: ProposalStatus.approved, employeeId: 'm9'));
        });
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await loaded(bloc);
        expect(bloc.state.proposals.single.name, 'Sam');
        expect(bloc.state.badge, 1);
        bloc.add(ProposalDecided(bloc.state.proposals.single, approve: true));
        await stateWhere(bloc, (s) => s.proposals.isEmpty);
      },
      verify: (bloc) {
        expect(bloc.state.proposals, isEmpty);
        final request = verify(() => repo.write(captureAny(), retry: false)).captured.single as Map<String, dynamic>;
        expect(request['method'], 'proposal.decide');
        expect(params(request), {'approve': true, 'proposal_id': 'pr1'});
      },
    );

    blocTest<InboxBloc, InboxState>(
      'declining a proposal sends the note',
      build: () {
        when(() => repo.approvals()).thenAnswer((_) async => []);
        when(() => repo.notifications()).thenAnswer((_) async => const NotificationsSnapshot(notifications: [], unread: 0));
        when(() => repo.proposals()).thenAnswer((_) async => [proposalOf('pr1')]);
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async => receipt(const CommandResultProposalDecided(proposalId: 'pr1', status: ProposalStatus.rejected)));
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await loaded(bloc);
        bloc.add(ProposalDecided(bloc.state.proposals.single, approve: false, note: 'Not now'));
        await stateWhere(bloc, (s) => s.notice == 'Declined.');
      },
      verify: (_) {
        final request = verify(() => repo.write(captureAny(), retry: false)).captured.single as Map<String, dynamic>;
        expect(params(request), {'approve': false, 'note': 'Not now', 'proposal_id': 'pr1'});
      },
      expect: () => contains(isA<InboxState>().having((s) => s.notice, 'notice', 'Declined.')),
    );

    blocTest<InboxBloc, InboxState>(
      'a decision decided elsewhere reads "changed or already handled" and the card goes away',
      build: () {
        var pending = [approval('a1')];
        when(() => repo.approvals()).thenAnswer((_) async => pending);
        // Unread: a failure, a finished result and the alert of the approval itself; only the failure adds to the badge.
        when(() => repo.notifications()).thenAnswer((_) async => NotificationsSnapshot(
              notifications: [
                note('f1', severity: 'warning', targetType: 'task.failed'),
                note('r1', targetType: 'task.succeeded'),
                note('ap', severity: 'action', targetType: 'approval.requested'),
              ],
              unread: 3,
            ));
        when(() => repo.proposals()).thenAnswer((_) async => []);
        when(() => repo.write(any(), retry: false)).thenAnswer((_) async {
          pending = [];
          throw const ProtocolException(ErrorCode.validation, 'This approval was already decided.');
        });
        return InboxBloc(repo);
      },
      act: (bloc) async {
        await loaded(bloc);
        expect(bloc.state.badge, 2, reason: 'the approval and the failed task; a finished result gives no badge');
        expect(bloc.state.blocking('conv-1')?.id, 'a1');
        bloc.add(ApprovalDecided(bloc.state.approvals.single, approve: true));
        await stateWhere(bloc, (s) => s.approvals.isEmpty && s.deciding.isEmpty);
      },
      verify: (bloc) {
        expect(bloc.state.approvals, isEmpty);
        expect(bloc.state.deciding, isEmpty);
      },
      expect: () => contains(isA<InboxState>().having((s) => s.notice, 'notice', 'This request changed or was already handled.')),
    );

    group('Harness²', () {
      const harness2 = LinkState(status: LinkStatus.online, capabilities: ['read', 'approve'], features: [Harness2.instructionProposals, Harness2.variants, Harness2.lessons]);
      void quiet() {
        when(() => repo.approvals()).thenAnswer((_) async => []);
        when(() => repo.notifications()).thenAnswer((_) async => const NotificationsSnapshot(notifications: [], unread: 0));
      }

      blocTest<InboxBloc, InboxState>(
        'proposed instructions and variants stay hidden on a server without the features',
        build: () {
          quiet();
          when(() => repo.proposals()).thenAnswer((_) async => [instructionProposalOf('pr2'), proposalOf('pr1')]);
          return InboxBloc(repo);
        },
        act: (bloc) async {
          await loaded(bloc);
          bloc.add(const VariantsRequested('t1'));
          await Future<void>.delayed(const Duration(milliseconds: 20));
        },
        verify: (bloc) {
          expect(bloc.state.proposals.map((p) => p.id), ['pr1'], reason: 'the colleague stays, the instructions go');
          expect(bloc.state.variantsOn, isFalse);
          expect(bloc.state.variants, isEmpty);
          verifyNever(() => repo.variantsOf(any()));
        },
      );

      blocTest<InboxBloc, InboxState>(
        'approving proposed instructions with edits sends the edited text',
        build: () {
          when(() => repo.linkState).thenReturn(harness2);
          quiet();
          var pending = [instructionProposalOf('pr2')];
          when(() => repo.proposals()).thenAnswer((_) async => pending);
          when(() => repo.write(any(), retry: false)).thenAnswer((_) async {
            pending = [];
            return receipt(const CommandResultProposalDecided(proposalId: 'pr2', status: ProposalStatus.approved));
          });
          return InboxBloc(repo);
        },
        act: (bloc) async {
          await loaded(bloc);
          expect(bloc.state.proposals.single.isInstructions, isTrue);
          bloc.add(ProposalDecided(bloc.state.proposals.single, approve: true, instructions: 'Be brief.\nRun all tests.'));
          await stateWhere(bloc, (s) => s.proposals.isEmpty);
        },
        verify: (bloc) {
          final request = verify(() => repo.write(captureAny(), retry: false)).captured.single as Map<String, dynamic>;
          expect(request['method'], 'proposal.decide');
          expect(params(request), {'approve': true, 'instructions': 'Be brief.\nRun all tests.', 'proposal_id': 'pr2'});
        },
        expect: () => contains(isA<InboxState>().having((s) => s.notice, 'notice', 'Your edited instructions for Max are saved.')),
      );

      blocTest<InboxBloc, InboxState>(
        'a stale base version reads as a conflict; rejecting sends no instructions',
        build: () {
          when(() => repo.linkState).thenReturn(harness2);
          quiet();
          when(() => repo.proposals()).thenAnswer((_) async => [instructionProposalOf('pr2', baseVersion: 2, currentVersion: 3)]);
          when(() => repo.write(any(), retry: false)).thenAnswer((invocation) async {
            final request = invocation.positionalArguments.first as Map<String, dynamic>;
            if (params(request)['approve'] == true) throw const ProtocolException(ErrorCode.versionConflict, 'stale');
            return receipt(const CommandResultProposalDecided(proposalId: 'pr2', status: ProposalStatus.rejected));
          });
          return InboxBloc(repo);
        },
        act: (bloc) async {
          await loaded(bloc);
          final p = bloc.state.proposals.single;
          expect(p.isStale, isTrue);
          bloc.add(ProposalDecided(p, approve: true, instructions: 'x'));
          await stateWhere(bloc, (s) => s.notice?.contains('changed after this proposal') ?? false);
          bloc.add(ProposalDecided(p, approve: false, note: 'Old', instructions: 'ignored'));
          await stateWhere(bloc, (s) => s.notice?.startsWith('Rejected.') ?? false);
        },
        verify: (_) {
          final requests = verify(() => repo.write(captureAny(), retry: false)).captured.cast<Map<String, dynamic>>();
          expect(params(requests.last), {'approve': false, 'note': 'Old', 'proposal_id': 'pr2'});
        },
      );

      blocTest<InboxBloc, InboxState>(
        'variants load for the compare card, refresh on their events and a choice is sent',
        build: () {
          when(() => repo.linkState).thenReturn(harness2);
          quiet();
          when(() => repo.proposals()).thenAnswer((_) async => []);
          final changes = StreamController<Event>.broadcast();
          addTearDown(changes.close);
          when(() => repo.companyChanges).thenAnswer((_) => changes.stream);
          var phase = VariantPhase.ready;
          when(() => repo.variantsOf('t1')).thenAnswer((_) async => variantsOf('t1', phase: phase));
          when(() => repo.write(any(), retry: false)).thenAnswer((_) async {
            phase = VariantPhase.applying;
            return receipt(const CommandResultTaskVariantChosen(taskId: 't1', variant: 1));
          });
          return InboxBloc(repo);
        },
        act: (bloc) async {
          await loaded(bloc);
          expect(bloc.state.variantsOn, isTrue);
          bloc.add(const VariantsRequested('t1'));
          await stateWhere(bloc, (s) => s.variants['t1'] != null);
          expect(bloc.state.variants['t1']!.judgePick, 2);
          bloc.add(const VariantChosen('t1', variant: 1));
          await stateWhere(bloc, (s) => s.variants['t1']?.phase == VariantPhase.applying && s.deciding.isEmpty);
        },
        verify: (bloc) {
          final request = verify(() => repo.write(captureAny(), retry: false)).captured.single as Map<String, dynamic>;
          expect(request['method'], 'task.choose_variant');
          expect(params(request), {'task_id': 't1', 'variant': 1});
        },
        expect: () => contains(isA<InboxState>().having((s) => s.notice, 'notice', 'Variant 1 is applied.')),
      );

      blocTest<InboxBloc, InboxState>(
        "letting the judge choose sends no variant; settled elsewhere says so",
        build: () {
          when(() => repo.linkState).thenReturn(harness2);
          quiet();
          when(() => repo.proposals()).thenAnswer((_) async => []);
          when(() => repo.variantsOf('t1')).thenAnswer((_) async => variantsOf('t1', phase: VariantPhase.done));
          when(() => repo.write(any(), retry: false)).thenThrow(const ProtocolException(ErrorCode.validation, 'not ready'));
          return InboxBloc(repo);
        },
        act: (bloc) async {
          await loaded(bloc);
          bloc.add(const VariantChosen('t1'));
          await stateWhere(bloc, (s) => s.notice != null);
        },
        verify: (_) {
          final request = verify(() => repo.write(captureAny(), retry: false)).captured.single as Map<String, dynamic>;
          expect(params(request), {'task_id': 't1'});
        },
        expect: () => contains(isA<InboxState>().having((s) => s.notice, 'notice', 'These variants were already settled.')),
      );

      test('Needs you shows tasks with variants ready as a compare card, only with the feature', () {
        final ready = taskOf(id: 'tv', status: TaskStatus.waiting, waitingReason: 'variants', variantPhase: VariantPhase.ready);
        final input = taskOf(id: 'ti', status: TaskStatus.waiting, waitingReason: 'input');
        final company = CompanyState(tasks: [ready, input]);
        final on = InboxSections.of(const InboxState(variantsOn: true), company);
        expect(on.variantTasks.map((t) => t.id), ['tv']);
        expect(on.waitingTasks.map((t) => t.id), ['ti']);
        expect(on.needsYou, 2);
        final off = InboxSections.of(const InboxState(), company);
        expect(off.variantTasks, isEmpty);
        expect(off.waitingTasks.map((t) => t.id), containsAll(['tv', 'ti']), reason: 'without the feature it stays a plain waiting task');
      });
    });
  });

  group('UpdatesBloc', () {
    UpdateStatus status(String current, {String? staged, bool available = true}) =>
        UpdateStatus(current: current, enabled: true, latest: '0.3.1', available: available, staged: staged, checkedAtMs: 1);

    blocTest<UpdatesBloc, UpdatesState>(
      'installing from the phone waits for the restart, then reports the new version',
      build: () {
        final online = StreamController<void>.broadcast();
        var current = status('0.3.0');
        when(() => repo.online).thenAnswer((_) => online.stream);
        when(() => repo.updateStatus(check: any(named: 'check'))).thenAnswer((_) async => current);
        when(() => repo.prepareUpdate()).thenAnswer((_) async => status('0.3.0', staged: '0.3.1'));
        when(() => repo.applyUpdate()).thenAnswer((_) async {
          // The server stops right after answering; the answer is lost here.
          current = status('0.3.1', available: false);
          Future<void>.delayed(const Duration(milliseconds: 50), () => online.add(null));
          throw const DisconnectedException();
        });
        return UpdatesBloc(repo);
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 20));
        bloc.add(const UpdateInstallRequested());
      },
      wait: const Duration(milliseconds: 200),
      verify: (bloc) {
        expect(bloc.state.phase, UpdatePhase.idle);
        expect(bloc.state.status?.current, '0.3.1');
        expect(bloc.state.notice, 'The server now runs Aurelius 0.3.1.');
      },
    );

    blocTest<UpdatesBloc, UpdatesState>(
      'nothing to install says so and installs nothing',
      build: () {
        when(() => repo.updateStatus(check: any(named: 'check'))).thenAnswer((_) async => status('0.3.0', available: false));
        when(() => repo.prepareUpdate()).thenAnswer((_) async => status('0.3.0', available: false));
        return UpdatesBloc(repo);
      },
      act: (bloc) => bloc.add(const UpdateInstallRequested()),
      wait: const Duration(milliseconds: 100),
      verify: (bloc) {
        expect(bloc.state.phase, UpdatePhase.idle);
        expect(bloc.state.notice, 'No newer version available.');
        verifyNever(() => repo.applyUpdate());
      },
    );

    blocTest<UpdatesBloc, UpdatesState>(
      'restarting the server waits until it is back, then says so',
      build: () {
        final online = StreamController<void>.broadcast();
        final link = StreamController<LinkState>.broadcast();
        when(() => repo.online).thenAnswer((_) => online.stream);
        when(() => repo.link).thenAnswer((_) => link.stream);
        when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.online));
        when(() => repo.updateStatus(check: any(named: 'check'))).thenAnswer((_) async => status('0.3.0'));
        when(() => repo.restartServer()).thenAnswer((_) async {
          // The server answers, closes the connection and comes back.
          Future<void>.delayed(const Duration(milliseconds: 10), () => link.add(const LinkState(status: LinkStatus.connecting)));
          Future<void>.delayed(const Duration(milliseconds: 60), () => online.add(null));
        });
        return UpdatesBloc(repo);
      },
      act: (bloc) async {
        await Future<void>.delayed(const Duration(milliseconds: 20));
        bloc.add(const ServerRestartRequested());
      },
      wait: const Duration(milliseconds: 200),
      expect: () => containsAllInOrder([
        isA<UpdatesState>().having((s) => s.phase, 'phase', UpdatePhase.stopping),
        isA<UpdatesState>().having((s) => s.phase, 'phase', UpdatePhase.rebooting),
        isA<UpdatesState>().having((s) => s.notice, 'notice', 'The server restarted.'),
      ]),
      verify: (bloc) => expect(bloc.state.phase, UpdatePhase.idle),
    );

    blocTest<UpdatesBloc, UpdatesState>(
      'a server that never answers and stays connected did not restart',
      build: () {
        when(() => repo.link).thenAnswer((_) => const Stream.empty());
        when(() => repo.linkState).thenReturn(const LinkState(status: LinkStatus.online));
        when(() => repo.updateStatus(check: any(named: 'check'))).thenAnswer((_) async => status('0.3.0'));
        when(() => repo.restartServer()).thenThrow(const DisconnectedException('The PC did not answer in time.'));
        return UpdatesBloc(repo, dropTimeout: const Duration(milliseconds: 50));
      },
      act: (bloc) => bloc.add(const ServerRestartRequested()),
      wait: const Duration(milliseconds: 200),
      verify: (bloc) {
        expect(bloc.state.phase, UpdatePhase.idle);
        expect(bloc.state.notice, 'The server did not restart.');
      },
    );
  });

  group('PairingBloc', () {
    const payload = PairingPayload(url: 'https://pc.tail1.ts.net:8443', code: 'K7M29QXA', hostId: '', hostName: 'my-pc', environment: 'production');

    blocTest<PairingBloc, PairingState>(
      'waits for the desktop, then yields the host and the secret',
      build: () {
        final api = MockApi();
        var polls = 0;
        when(() => api.pair(payload, deviceName: 'iPhone', platform: 'ios')).thenAnswer((_) async => 'p1');
        when(() => api.pollPairing(payload.url, 'p1')).thenAnswer((_) async => polls++ == 0
            ? const PairingPending()
            : const PairingApproved(clientId: 'c1', deviceSecret: 's3cret', hostId: 'h1', environment: 'production'));
        return PairingBloc(api, platform: 'ios');
      },
      act: (bloc) => bloc.add(const PairingSubmitted(payload, deviceName: 'iPhone')),
      wait: const Duration(milliseconds: 50),
      expect: () => [
        isA<PairingState>().having((s) => s.step, 'step', PairingStep.submitting),
        isA<PairingState>().having((s) => s.step, 'step', PairingStep.waitingForDesktop),
        isA<PairingState>()
            .having((s) => s.step, 'step', PairingStep.paired)
            .having((s) => s.host?.hostId, 'host', 'h1')
            .having((s) => s.host?.clientId, 'client', 'c1')
            .having((s) => s.deviceSecret, 'secret', 's3cret'),
      ],
    );

    blocTest<PairingBloc, PairingState>(
      'a refusal on the desktop is reported',
      build: () {
        final api = MockApi();
        when(() => api.pair(payload, deviceName: 'x', platform: 'ios')).thenAnswer((_) async => 'p1');
        when(() => api.pollPairing(payload.url, 'p1')).thenAnswer((_) async => const PairingRejected());
        return PairingBloc(api, platform: 'ios');
      },
      act: (bloc) => bloc.add(const PairingSubmitted(payload, deviceName: 'x')),
      wait: const Duration(milliseconds: 50),
      verify: (bloc) {
        expect(bloc.state.step, PairingStep.failed);
        expect(bloc.state.message, 'The PC refused this phone.');
      },
    );
  });
}
