import 'dart:async';

import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/channels.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/links.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:aurelius_mobile/features/team/channel_link_page.dart';
import 'package:aurelius_mobile/features/team/channels_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart' as board;

// A5a: where a link lands in a chat: the message, far up the list, marked; the
// thread of a reply; Copy link on a long press.

class _MockRepository extends Mock implements AureliusRepository {}

class _MockInboxBloc extends MockBloc<InboxEvent, InboxState> implements InboxBloc {}

class _MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

class _MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

class _MockRuntimeBloc extends MockBloc<RuntimeEvent, RuntimeState> implements RuntimeBloc {}

const _hostId = '01a19bd5-6957-73a6-a074-d98f1c80f0b2';
const _host = Host(hostId: _hostId, name: 'Segrusa', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0);
const _ada = UserView(id: 'u-1', username: 'ada', createdAtMs: 0);

LinkState _link({bool links = true}) => LinkState(
      status: LinkStatus.online,
      capabilities: const ['read', 'chat', 'task_control'],
      features: [ticketDiscussionFeature, teamChannelsFeature, ticketChannelsFeature, chatThreadsFeature, if (links) messageLocateFeature],
      boardMembersExist: true,
      user: _ada,
    );

const _channel = ChannelView(archived: false, conversationId: 'c-42', kind: ChannelKind.project, mentions: 0, name: 'aurelius', projectId: 'p1', unread: 0);

MessageView _msg(String id, String text, {int at = 1000, int ordinal = 0, String? threadParent}) => MessageView(
      activity: const [],
      authorUser: _ada,
      createdAtMs: at,
      files: const [],
      id: id,
      ordinal: ordinal,
      queued: false,
      reactions: const [],
      role: MessageRole.user,
      sources: const [],
      status: MessageStatus.complete,
      text: text,
      threadParent: threadParent,
      uploads: const [],
    );

final _many = [for (var i = 0; i < 80; i++) _msg('m$i', 'Message number $i\nwith a second line', at: 1000 + i, ordinal: i)];

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  Future<_MockRepository> pump(WidgetTester tester, Widget page, {LinkState? link, List<MessageView>? messages, List<ChannelView> channels = const [_channel], ThreadSnapshot? thread, TicketDetail? detail}) async {
    tester.view.physicalSize = const Size(390, 844) * 3;
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.reset);
    final state = link ?? _link();
    final repo = _MockRepository();
    when(() => repo.host).thenReturn(_host);
    when(() => repo.linkState).thenReturn(state);
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.changes).thenAnswer((_) => const Stream.empty());
    when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.boardChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.appends).thenAnswer((_) => const Stream.empty());
    when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.link).thenAnswer((_) => const Stream.empty());
    when(() => repo.load()).thenAnswer((_) async => loadOf());
    when(() => repo.channels()).thenAnswer((_) async => channels);
    when(() => repo.boardMembers()).thenAnswer((_) async => const [_ada]);
    when(() => repo.groupChat(channel: any(named: 'channel'))).thenAnswer((_) async => snapshotOf('c-42', messages: messages ?? _many));
    when(() => repo.messages(any())).thenAnswer((_) async => snapshotOf('c-42', messages: messages ?? _many));
    if (thread != null) when(() => repo.thread(any())).thenAnswer((_) async => thread);
    if (detail != null) when(() => repo.ticket(any())).thenAnswer((_) async => detail);
    when(() => repo.timeline(correlationId: any(named: 'correlationId'), activity: any(named: 'activity'), limit: any(named: 'limit'))).thenAnswer((_) async => const TimelinePage(cards: []));
    final company = _MockCompanyBloc();
    whenListen(company, const Stream<CompanyState>.empty(), initialState: const CompanyState(loading: false));
    final inbox = _MockInboxBloc();
    whenListen(inbox, const Stream<InboxState>.empty(), initialState: const InboxState(loading: false));
    final connection = _MockConnectionBloc();
    whenListen(connection, const Stream<LinkState>.empty(), initialState: state);
    final runtime = _MockRuntimeBloc();
    whenListen(runtime, const Stream<RuntimeState>.empty(), initialState: const RuntimeState());
    final router = GoRouter(initialLocation: '/', routes: [
      GoRoute(path: '/', builder: (_, _) => page),
      GoRoute(path: '/board', builder: (_, _) => const Scaffold(body: Text('the board'))),
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
          BlocProvider<ChannelsBloc>(create: (_) => ChannelsBloc(repo)),
        ],
        child: MaterialApp.router(theme: aureliusTheme(), routerConfig: router),
      ),
    ));
    return repo;
  }

  Future<void> settle(WidgetTester tester, {int frames = 40}) async {
    for (var i = 0; i < frames; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
  }

  /// Until [key] shows: the mark lasts only 1.5 s, so a fixed wait could pass it.
  Future<void> untilMarked(WidgetTester tester, String key) async {
    for (var i = 0; i < 60 && find.byKey(ValueKey(key)).evaluate().isEmpty; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    // Let it scroll there.
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 100));
  }

  group('a link to a message in a channel', () {
    testWidgets('scrolls up to a message far from the end and marks it', (tester) async {
      await pump(tester, const ChannelLinkPage(conversationId: 'c-42', message: 'm3'));
      await untilMarked(tester, 'flash-m3');
      expect(find.byKey(const ValueKey('flash-m3')), findsOneWidget);
      expect(tester.getTopLeft(find.byKey(const ValueKey('flash-m3'))).dy, inInclusiveRange(0, 844));
      expect(find.textContaining('Message number 79', findRichText: true), findsNothing, reason: 'the end of the list is out of sight');
    });

    testWidgets('the mark goes after 1.5 s; the list then stays where it is', (tester) async {
      await pump(tester, const ChannelLinkPage(conversationId: 'c-42', message: 'm3'));
      await untilMarked(tester, 'flash-m3');
      expect(find.byKey(const ValueKey('flash-m3')), findsOneWidget);
      await settle(tester);
      expect(find.byKey(const ValueKey('flash-m3')), findsNothing);
      expect(find.textContaining('Message number 3', findRichText: true), findsWidgets);
    });

    testWidgets('a message that is gone says so; the channel still opens', (tester) async {
      await pump(tester, const ChannelLinkPage(conversationId: 'c-42', message: 'gone'));
      await settle(tester, frames: 8);
      expect(find.text('That message no longer exists.'), findsOneWidget);
      expect(find.byKey(const ValueKey('team-chat-c-42')), findsOneWidget);
    });

    testWidgets('a reply opens its thread as well, on that reply', (tester) async {
      final root = _msg('m1', 'The root');
      await pump(
        tester,
        const ChannelLinkPage(conversationId: 'c-42', message: 'r2', thread: 'm1'),
        messages: [root],
        thread: ThreadSnapshot(asOfSeq: 1, conversationId: 'c-42', root: root, replies: [_msg('r1', 'One', ordinal: 2, threadParent: 'm1'), _msg('r2', 'Two', ordinal: 3, threadParent: 'm1')]),
      );
      await untilMarked(tester, 'flash-r2');
      expect(find.byKey(const ValueKey('thread-title')), findsOneWidget);
      expect(find.byKey(const ValueKey('flash-r2')), findsOneWidget);
    });

    testWidgets('a channel that does not exist says so and leads to the Board', (tester) async {
      await pump(tester, const ChannelLinkPage(conversationId: 'c-unknown', message: 'm1'), channels: const [_channel]);
      await settle(tester, frames: 8);
      expect(find.text('This channel is not available.'), findsOneWidget);
      await tester.tap(find.text('Go to Board'));
      await settle(tester, frames: 8);
      expect(find.text('the board'), findsOneWidget);
    });
  });

  testWidgets('a link to a message in a ticket opens the discussion on it', (tester) async {
    final ticket = board.ticketOf(id: 'k1', number: 42, column: ColumnKind.doing);
    final withChat = TicketView.fromJson({...ticket.toJson(), 'conversation_id': 'c-42'});
    await pump(tester, const TicketPage(ticketId: 'k1', discussion: true, message: 'm3'), detail: TicketDetail(asOfSeq: 1, ticket: withChat, tasks: const []));
    await untilMarked(tester, 'flash-m3');
    expect(find.byKey(const ValueKey('flash-m3')), findsOneWidget);
  });

  group('the header of a ticket\'s channel (§1.4)', () {
    const ticketChannel = ChannelView(archived: false, conversationId: 'c-42', kind: ChannelKind.ticket, mentions: 0, name: '#42-login', projectId: 'p1', ticketColumn: ColumnKind.review, ticketId: 'k1', ticketNumber: 42, unread: 0);

    testWidgets('names who is responsible', (tester) async {
      final ticket = board.ticketOf(id: 'k1', number: 42, column: ColumnKind.review, responsible: board.bo);
      await pump(tester, const ChannelLinkPage(conversationId: 'c-42'), channels: const [ticketChannel], messages: const [], detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []));
      await settle(tester, frames: 8);
      expect(find.text('bo · responsible'), findsOneWidget);
    });

    testWidgets('a server without a responsible shows no such line', (tester) async {
      final ticket = board.ticketOf(id: 'k1', number: 42, column: ColumnKind.review);
      await pump(tester, const ChannelLinkPage(conversationId: 'c-42'), channels: const [ticketChannel], messages: const [], detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []));
      await settle(tester, frames: 8);
      expect(find.byKey(const ValueKey('channel-ticket-responsible')), findsNothing);
    });
  });

  group('Copy link on a message (§6.1)', () {
    String? clipboard;

    setUp(() {
      clipboard = null;
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
        if (call.method == 'Clipboard.setData') clipboard = (call.arguments as Map)['text'] as String?;
        return null;
      });
    });

    tearDown(() => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, null));

    Future<void> longPress(WidgetTester tester) async {
      await settle(tester, frames: 8);
      await tester.longPress(find.textContaining('The message', findRichText: true));
      await settle(tester, frames: 8);
    }

    testWidgets('a long press offers it after Copy, and the toast names it', (tester) async {
      await pump(tester, const ChannelLinkPage(conversationId: 'c-42'), messages: [_msg('01a19625-8790-74ca-807b-758adf34b197', 'The message')]);
      await longPress(tester);
      await tester.tap(find.text('Copy link'));
      await settle(tester, frames: 8);
      expect(clipboard, 'aurelius://$_hostId/message/01a19625-8790-74ca-807b-758adf34b197');
      expect(find.text('Link to message copied.'), findsOneWidget);
    });

    testWidgets('an older server does not offer it', (tester) async {
      await pump(tester, const ChannelLinkPage(conversationId: 'c-42'), link: _link(links: false), messages: [_msg('m1', 'The message')]);
      await longPress(tester);
      expect(find.text('Copy link'), findsNothing);
    });
  });
}
