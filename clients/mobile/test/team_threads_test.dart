import 'dart:async';

import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/theme/tokens.dart';
import 'package:aurelius_mobile/core/widgets/widgets.dart' show Monogram;
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/channels.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:aurelius_mobile/features/team/channels_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/channel_opener.dart';
import 'support/tickets.dart' as board;

// AUR-CHAT-002-M1: threads and reactions in the Team chat on a phone, with a
// server of `chat_threads` and `chat_reactions` (ui-ontwerp §5, §6). Neutral
// names: board members ada (signed in) and lena; employees Orion and Forge.

class MockRepository extends Mock implements AureliusRepository {}

class MockInboxBloc extends MockBloc<InboxEvent, InboxState> implements InboxBloc {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

class MockRuntimeBloc extends MockBloc<RuntimeEvent, RuntimeState> implements RuntimeBloc {}

const _ada = UserView(id: 'u-1', username: 'ada', createdAtMs: 0);
const _lena = UserView(id: 'u-2', username: 'lena', createdAtMs: 0);
const _host = Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0);

LinkState _link({required bool threads, required bool reactions}) => LinkState(
      status: LinkStatus.online,
      capabilities: const ['read', 'chat', 'task_control'],
      features: [ticketDiscussionFeature, teamChannelsFeature, ticketChannelsFeature, if (threads) chatThreadsFeature, if (reactions) chatReactionsFeature],
      boardMembersExist: true,
      user: _ada,
    );

ChannelView _channel(String id, String name, {bool archived = false, int number = 42, ColumnKind column = ColumnKind.review}) => ChannelView(
      archived: archived,
      conversationId: id,
      kind: ChannelKind.ticket,
      lastActivityMs: 0,
      mentions: 0,
      name: name,
      projectId: 'p1',
      ticketColumn: column,
      ticketId: 'k$number',
      ticketNumber: number,
      unread: 0,
    );

final _general = ChannelView(archived: false, conversationId: 'c-general', kind: ChannelKind.general, mentions: 0, name: 'General', unread: 0);
final _t42 = _channel('c-42', '#42-login-per-board-member');
final _t47 = _channel('c-47', '#47-vault-rotation', archived: true, number: 47, column: ColumnKind.done);

/// A time today, [minutes] after the start of this test's hour window (never yesterday).
int _today(int minutes) {
  final now = DateTime.now();
  final start = now.hour == 0 && now.minute < 30 ? DateUtils.dateOnly(now) : now.subtract(const Duration(minutes: 30));
  return start.add(Duration(minutes: minutes)).millisecondsSinceEpoch;
}

MessageView _msg(String id, String text, {UserView? by, String? author, required int at, int ordinal = 0, String? threadParent, int? replies, int? lastMs, int? unread, List<String> repliers = const [], List<ReactionView> reactions = const []}) => MessageView(
      activity: const [],
      author: author,
      authorUser: by,
      createdAtMs: at,
      files: const [],
      id: id,
      ordinal: ordinal,
      queued: false,
      reactions: reactions,
      role: by != null ? MessageRole.user : MessageRole.assistant,
      runId: by != null ? null : 'run-$id',
      sources: const [],
      status: MessageStatus.complete,
      text: text,
      threadLastReplyMs: lastMs,
      threadParent: threadParent,
      threadRepliers: repliers,
      threadReplyCount: replies,
      threadUnread: unread,
      update: by != null ? null : 'group',
      uploads: const [],
    );

const _tick = ReactionView(emoji: '✓', count: 2, mine: true, by: ['ada', 'lena']);
const _eyes = ReactionView(emoji: '👀', count: 1, mine: false, by: ['Forge']);

final _root = _msg('m1', 'Please keep the argon2 params configurable.', by: _lena, at: _today(0), ordinal: 1, replies: 3, lastMs: _today(5), unread: 2, repliers: const ['Orion', 'lena', 'Forge'], reactions: const [_tick, _eyes]);
final _plain = _msg('m2', 'Done. The cost is in config.', author: 'Forge', at: _today(10), ordinal: 2);
final _replies = [
  _msg('r1', 'Which ones exactly?', author: 'Forge', at: _today(2), ordinal: 3, threadParent: 'm1'),
  _msg('r2', 'Memory and iterations.', by: _lena, at: _today(3), ordinal: 4, threadParent: 'm1'),
  _msg('r3', 'On it.', author: 'Orion', at: _today(5), ordinal: 5, threadParent: 'm1'),
];

ThreadSnapshot _thread({bool readOnly = false, String conversation = 'c-42', MessageView? root, List<MessageView>? replies}) =>
    ThreadSnapshot(asOfSeq: 1, conversationId: conversation, root: root ?? _root, replies: replies ?? _replies, readOnly: readOnly ? true : null);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  late MockRepository repo;
  late StreamController<Event> chat;
  late StreamController<StreamAppend> appends;

  /// Every write, in order (a test may expect none, which `verify` cannot).
  late List<Map<String, dynamic>> sent;

  Future<void> pump(
    WidgetTester tester, {
    bool threads = true,
    bool reactions = true,
    List<ChannelView>? list,
    Map<String?, List<MessageView>>? messages,
    ThreadSnapshot? thread,
    Object? writeError,
  }) async {
    tester.view.physicalSize = const Size(390, 844) * 3;
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.reset);
    final link = _link(threads: threads, reactions: reactions);
    sent = [];
    chat = StreamController<Event>.broadcast();
    appends = StreamController<StreamAppend>.broadcast();
    addTearDown(chat.close);
    addTearDown(appends.close);
    final all = messages ?? {'c-42': [_root, _plain], 'c-47': [_root.withReactions(const [_tick])]};
    repo = MockRepository();
    when(() => repo.host).thenReturn(_host);
    when(() => repo.linkState).thenReturn(link);
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.changes).thenAnswer((_) => const Stream.empty());
    when(() => repo.chatChanges).thenAnswer((_) => chat.stream);
    when(() => repo.boardChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.appends).thenAnswer((_) => appends.stream);
    when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.load()).thenAnswer((_) async => loadOf());
    when(() => repo.channels()).thenAnswer((_) async => list ?? [_general, _t42, _t47]);
    when(() => repo.boardMembers()).thenAnswer((_) async => const [_ada, _lena]);
    when(() => repo.ticket(any())).thenAnswer((i) async => TicketDetail(asOfSeq: 1, ticket: board.ticketOf(id: i.positionalArguments.first as String, column: ColumnKind.review), tasks: const []));
    when(() => repo.groupChat(channel: any(named: 'channel'))).thenAnswer((i) async {
      final channel = i.namedArguments[#channel] as String?;
      return snapshotOf(channel ?? 'c-general', messages: all[channel] ?? const []);
    });
    when(() => repo.thread(any())).thenAnswer((_) async => thread ?? _thread());
    when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenAnswer((i) async {
      final request = i.positionalArguments.first as Map<String, dynamic>;
      sent.add(request);
      if (writeError != null) throw writeError;
      if (request['method'] == 'message.react') {
        final p = paramsOf(request);
        return receiptOf(CommandResultMessageReacted(changed: true, conversationId: 'c-42', emoji: p['emoji'] as String, messageId: p['message_id'] as String, on: p['on'] as bool));
      }
      return receiptOf(const CommandResultGroupMessageSent(conversationId: 'c-42', messageId: 'm-new', runIds: [], to: []));
    });
    final company = MockCompanyBloc();
    whenListen(company, const Stream<CompanyState>.empty(), initialState: CompanyState(loading: false, employees: [board.orion, board.forge]));
    final inbox = MockInboxBloc();
    whenListen(inbox, const Stream<InboxState>.empty(), initialState: const InboxState(loading: false));
    final connection = MockConnectionBloc();
    whenListen(connection, const Stream<LinkState>.empty(), initialState: link);
    final runtime = MockRuntimeBloc();
    whenListen(runtime, const Stream<RuntimeState>.empty(), initialState: const RuntimeState());
    final router = GoRouter(initialLocation: '/', routes: [
      GoRoute(path: '/', builder: (_, _) => const ChannelOpener()),
      GoRoute(path: '/ticket/:tid', builder: (_, s) => Scaffold(body: Text('ticket ${s.pathParameters['tid']}'))),
    ]);
    addTearDown(router.dispose);
    await tester.pumpWidget(RepositoryProvider<AureliusRepository>.value(
      value: repo,
      child: MultiBlocProvider(
        providers: [
          BlocProvider<CompanyBloc>.value(value: company),
          BlocProvider<InboxBloc>.value(value: inbox),
          BlocProvider<ConnectionBloc>.value(value: connection),
          BlocProvider<RuntimeBloc>.value(value: runtime),
          BlocProvider(create: (_) => ChannelsBloc(repo)),
        ],
        child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router),
      ),
    ));
    await board.settleBoard(tester);
  }

  Future<void> open(WidgetTester tester, String id) async {
    await tester.tap(find.byKey(ValueKey('channel-$id')));
    await board.settleBoard(tester);
  }

  Future<void> openArchived(WidgetTester tester, String id) => open(tester, id);

  Future<void> openThread(WidgetTester tester) async {
    await tester.tap(find.byKey(const ValueKey('thread-line-m1')));
    await board.settleBoard(tester);
  }

  List<Map<String, dynamic>> writes(String method) => [for (final w in sent) if (w['method'] == method) w];

  group('compat: a server without chat_threads and chat_reactions', () {
    testWidgets('keeps the channel of phase 1: no thread line, no pills, no React or Reply in thread', (tester) async {
      await pump(tester, threads: false, reactions: false);
      await open(tester, 'c-42');
      expect(find.text('Please keep the argon2 params configurable.'), findsOneWidget);
      expect(find.byKey(const ValueKey('thread-line-m1')), findsNothing);
      expect(find.byKey(const ValueKey('reactions-m1')), findsNothing);
      await tester.longPress(find.text('Done. The cost is in config.'));
      await board.settleBoard(tester);
      expect(find.text('Create ticket'), findsOneWidget);
      expect(find.text('React'), findsNothing);
      expect(find.text('Reply in thread'), findsNothing);
      verifyNever(() => repo.thread(any()));
    });

    testWidgets('threads without reactions: the thread line, but no pills and no React', (tester) async {
      await pump(tester, reactions: false);
      await open(tester, 'c-42');
      expect(find.byKey(const ValueKey('thread-line-m1')), findsOneWidget);
      expect(find.byKey(const ValueKey('reactions-m1')), findsNothing);
      await tester.longPress(find.text('Done. The cost is in config.'));
      await board.settleBoard(tester);
      expect(find.text('Reply in thread'), findsOneWidget);
      expect(find.text('React'), findsNothing);
    });

    testWidgets("a thread's answer streaming in never shows in the channel", (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      appends.add(const StreamAppend(conversationId: 'c-42', messageId: 'live-1', runId: 'run-x', offset: 0, text: 'Thinking about the params', threadParent: 'm1'));
      await board.settleBoard(tester);
      expect(find.text('Thinking about the params'), findsNothing);
    });
  });

  group('threads (§5)', () {
    testWidgets('under a root: the reply count, the last reply, and how many are unread; replies stay out of the channel', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      expect(find.textContaining('↳ 3 replies · last '), findsOneWidget);
      expect(find.byKey(const ValueKey('thread-unread-m1')), findsOneWidget);
      expect(find.text('2 new'), findsOneWidget);
      expect(find.byKey(const ValueKey('thread-line-m2')), findsNothing, reason: 'a message without replies has no line');
      expect(find.text('Which ones exactly?'), findsNothing);
    });

    testWidgets('a tap on the line pushes "Thread · #channel" with the root on top, then the replies', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      await openThread(tester);
      verify(() => repo.thread('m1')).called(greaterThan(0));
      expect(find.byWidgetPredicate((w) => w is Text && w.key == const ValueKey('thread-title') && w.data == 'Thread · #42-login-per-board-member'), findsOneWidget);
      double top(Finder f) => tester.getTopLeft(f).dy;
      expect(top(find.text('Please keep the argon2 params configurable.')) < top(find.text('3 REPLIES')), isTrue);
      expect(top(find.text('3 REPLIES')) < top(find.text('Which ones exactly?')), isTrue);
      expect(find.text('On it.'), findsOneWidget);
      expect(find.byKey(const ValueKey('thread-line-m1')), findsNothing, reason: 'no thread line on the root inside its thread');
      expect(find.text('Reply… · @ to mention'), findsOneWidget);
    });

    testWidgets('a reply goes out with thread_parent in the channel; Return is a new line, the button sends', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      await openThread(tester);
      await tester.enterText(find.byType(TextField).last, 'Agreed @Forge');
      await tester.pump();
      await tester.testTextInput.receiveAction(TextInputAction.newline);
      await board.settleBoard(tester);
      expect(writes('group_chat.send'), isEmpty, reason: 'the on-screen Return is a new line');
      await tester.tap(find.byKey(const Key('composer-send')));
      await board.settleBoard(tester);
      final send = writes('group_chat.send').single;
      expect(paramsOf(send)['channel'], 'c-42');
      expect(paramsOf(send)['thread_parent'], 'm1');
      expect((paramsOf(send)['text'] as String).trim(), 'Agreed @Forge');
    });

    testWidgets('@ suggests who to mention in a thread too', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      await openThread(tester);
      await tester.enterText(find.byType(TextField).last, 'Ask @Fo');
      await tester.pump();
      await tester.tap(find.byKey(const Key('mention-Forge')));
      await tester.pump();
      expect(tester.widget<TextField>(find.byType(TextField).last).controller!.text, 'Ask @Forge ');
    });

    testWidgets('a long press on a message offers React, Reply in thread and Create ticket; Reply opens its thread', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      await tester.longPress(find.text('Done. The cost is in config.'));
      await board.settleBoard(tester);
      expect(find.text('React'), findsOneWidget);
      expect(find.text('Reply in thread'), findsOneWidget);
      expect(find.text('Create ticket'), findsOneWidget);
      await tester.tap(find.text('Reply in thread'));
      await board.settleBoard(tester);
      verify(() => repo.thread('m2')).called(greaterThan(0));
      expect(find.text('Reply… · @ to mention'), findsOneWidget);
    });

    testWidgets("the line shows who replied from the root's repliers, before the thread was ever opened", (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      final monograms = find.descendant(of: find.byKey(const ValueKey('thread-line-m1')), matching: find.byType(Monogram));
      expect(monograms, findsNWidgets(3), reason: 'Orion, lena and Forge, the latest first');
      final shown = tester.widgetList<Monogram>(monograms).toList();
      expect([for (final m in shown) m.letter], ['O', 'L', 'F']);
      expect([for (final m in shown) m.member], [false, true, false], reason: 'lena is no employee, so a board member');
      verifyNever(() => repo.thread('m1'));
    });

    testWidgets('back in the channel the counters are read again', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      await openThread(tester);
      clearInteractions(repo);
      Navigator.of(tester.element(find.byKey(const ValueKey('thread-title')))).pop();
      await board.settleBoard(tester);
      verify(() => repo.groupChat(channel: 'c-42')).called(greaterThan(0));
      expect(find.byKey(const ValueKey('thread-line-m1')), findsOneWidget);
    });

    testWidgets("a reply's event reads the open thread again; its answer streams into the thread", (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      await openThread(tester);
      clearInteractions(repo);
      chat.add(Event(aggregateId: 'c-42', atMs: 0, kind: EventKind.messageUserCommitted, seq: 20));
      await board.settleBoard(tester);
      verify(() => repo.thread('m1')).called(greaterThan(0));
      appends.add(const StreamAppend(conversationId: 'c-42', messageId: 'live-1', runId: 'run-x', offset: 0, text: 'Looking at the params', threadParent: 'm1'));
      await board.settleBoard(tester);
      expect(find.text('Looking at the params'), findsOneWidget);
      appends.add(const StreamAppend(conversationId: 'c-42', messageId: 'live-2', runId: 'run-y', offset: 0, text: 'Another thread', threadParent: 'm9'));
      await board.settleBoard(tester);
      expect(find.text('Another thread'), findsNothing, reason: "another thread's answer stays out");
    });
  });

  group('reactions (§5)', () {
    testWidgets('pills with their count; yours has the accent', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      final mine = tester.widget<Text>(find.text('✓ 2'));
      final other = tester.widget<Text>(find.text('👀 1'));
      expect(mine.style?.color, Tokens.accentText);
      expect(other.style?.color, Tokens.text2);
      expect(find.bySemanticsLabel('✓, 2 reactions, including yours'), findsOneWidget);
    });

    testWidgets('a tap on a pill turns yours off or on, shown at once', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      await tester.tap(find.byKey(const ValueKey('reaction-m1-✓')));
      await tester.pump();
      expect(find.text('✓ 1'), findsOneWidget);
      await board.settleBoard(tester);
      final off = writes('message.react').single;
      expect(paramsOf(off), {'message_id': 'm1', 'emoji': '✓', 'on': false});
      sent.clear();
      await tester.tap(find.byKey(const ValueKey('reaction-m1-👀')));
      await tester.pump();
      expect(find.text('👀 2'), findsOneWidget);
      await board.settleBoard(tester);
      expect(paramsOf(writes('message.react').single), {'message_id': 'm1', 'emoji': '👀', 'on': true});
    });

    testWidgets('the add pill opens the picker of exactly the eight reactions; a pick reacts', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      await tester.tap(find.byKey(const ValueKey('reaction-add-m1')));
      await board.settleBoard(tester);
      expect(find.byKey(const Key('reaction-picker')), findsOneWidget);
      for (final emoji in reactionChoices) {
        expect(find.byKey(ValueKey('reaction-pick-$emoji')), findsOneWidget);
      }
      expect(find.descendant(of: find.byKey(const Key('reaction-picker')), matching: find.byType(InkWell)), findsNWidgets(8));
      await tester.tap(find.byKey(const ValueKey('reaction-pick-🚀')));
      await board.settleBoard(tester);
      expect(paramsOf(writes('message.react').single), {'message_id': 'm1', 'emoji': '🚀', 'on': true});
      expect(find.text('🚀 1'), findsOneWidget);
    });

    testWidgets('React from a long press on a message without reactions', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      await tester.longPress(find.text('Done. The cost is in config.'));
      await board.settleBoard(tester);
      await tester.tap(find.text('React'));
      await board.settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('reaction-pick-👍')));
      await board.settleBoard(tester);
      expect(paramsOf(writes('message.react').single), {'message_id': 'm2', 'emoji': '👍', 'on': true});
    });

    testWidgets('a long press on a pill says who reacted', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      await tester.longPress(find.byKey(const ValueKey('reaction-m1-✓')));
      await tester.pump();
      expect(find.text('✓ · ada, lena'), findsOneWidget);
      expect(writes('message.react'), isEmpty);
    });

    testWidgets('a refused reaction goes back and says why', (tester) async {
      await pump(tester, writeError: const ProtocolException(ErrorCode.validation, 'This reaction is not one of the eight.'));
      await open(tester, 'c-42');
      await tester.tap(find.byKey(const ValueKey('reaction-m1-👀')));
      await board.settleBoard(tester);
      expect(find.text('👀 1'), findsOneWidget);
      expect(find.text('This reaction is not one of the eight.'), findsOneWidget);
    });

    testWidgets('message.reacted reads the channel again', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      clearInteractions(repo);
      chat.add(Event(aggregateId: 'c-42', atMs: 0, kind: EventKind.messageReacted, seq: 21));
      await board.settleBoard(tester);
      verify(() => repo.groupChat(channel: 'c-42')).called(greaterThan(0));
    });
  });

  group('closed channels (§3, §5)', () {
    testWidgets('nothing writable: no add pill, a tap toggles nothing, no React or Reply on a long press', (tester) async {
      await pump(tester);
      await openArchived(tester, 'c-47');
      expect(find.text('✓ 2'), findsOneWidget);
      expect(find.byKey(const ValueKey('reaction-add-m1')), findsNothing);
      await tester.tap(find.byKey(const ValueKey('reaction-m1-✓')));
      await board.settleBoard(tester);
      expect(writes('message.react'), isEmpty);
      await tester.longPress(find.text('Please keep the argon2 params configurable.'));
      await board.settleBoard(tester);
      expect(find.text('React'), findsNothing);
      expect(find.text('Reply in thread'), findsNothing);
    });

    testWidgets('its thread can be read, not answered (read_only)', (tester) async {
      await pump(tester, thread: _thread(readOnly: true, conversation: 'c-47', root: _root.withReactions(const [_tick])));
      await openArchived(tester, 'c-47');
      await openThread(tester);
      expect(find.text('Which ones exactly?'), findsOneWidget);
      expect(find.byKey(const Key('composer-send')), findsNothing);
      expect(find.byKey(const Key('chat-read-only')), findsOneWidget);
      expect(find.byKey(const ValueKey('reaction-add-m1')), findsNothing);
    });

    testWidgets('channel_archived on a reaction closes the channel, never shows the error', (tester) async {
      await pump(tester, writeError: const ProtocolException(ErrorCode.channelArchived, 'channel_archived: the ticket is on Done'));
      await open(tester, 'c-42');
      await tester.tap(find.byKey(const ValueKey('reaction-m1-👀')));
      await board.settleBoard(tester);
      expect(find.byKey(const Key('chat-read-only')), findsOneWidget);
      expect(find.textContaining('channel_archived'), findsNothing);
      expect(find.text('👀 1'), findsOneWidget);
    });
  });
}
