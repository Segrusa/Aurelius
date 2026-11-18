import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/widgets/ios.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/new_ticket_page.dart';
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:aurelius_mobile/features/team/channels_bloc.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/channel_opener.dart';
import 'support/tickets.dart' as board;
import 'ticket_discussion_test.dart' show lineOf, postOf;

// Phase 2, mobile: the Team chat's channels (MU2-08, B8), and a ticket made
// from a chat message (MU2-10). Neutral names: board members ada and lena.

class MockRepository extends Mock implements AureliusRepository {}

class MockInboxBloc extends MockBloc<InboxEvent, InboxState> implements InboxBloc {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

class MockRuntimeBloc extends MockBloc<RuntimeEvent, RuntimeState> implements RuntimeBloc {}

const _ada = UserView(id: 'u-1', username: 'ada', createdAtMs: 0);
const _lena = UserView(id: 'u-2', username: 'lena', createdAtMs: 0);
const _host = Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0);

LinkState linkWith({bool channels = true}) => LinkState(
      status: LinkStatus.online,
      capabilities: const ['read', 'chat', 'task_control'],
      features: [ticketDiscussionFeature, if (channels) teamChannelsFeature],
      boardMembersExist: true,
      user: _ada,
    );

ChannelView channelOf(String id, String name, {String? projectId, int unread = 0, int mentions = 0, bool archived = false}) =>
    ChannelView(archived: archived, conversationId: id, mentions: mentions, name: name, projectId: projectId, unread: unread, lastActivityMs: 0);

final _channels = [
  channelOf('c-general', 'General'),
  channelOf('c-web', 'Web', projectId: 'p2', unread: 1, mentions: 1),
  channelOf('c-atlas', 'Atlas', projectId: 'p1', unread: 3),
  channelOf('c-old', 'Old', projectId: 'p3', archived: true),
];

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  late MockRepository repo;

  /// A channel opener under a router that shows what "Create ticket" opens.
  Future<void> pump(WidgetTester tester, {bool channels = true, Map<String?, List<MessageView>> messages = const {}}) async {
    tester.view.physicalSize = const Size(390, 844) * 3;
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.reset);
    final link = linkWith(channels: channels);
    repo = MockRepository();
    when(() => repo.host).thenReturn(_host);
    when(() => repo.linkState).thenReturn(link);
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.changes).thenAnswer((_) => const Stream.empty());
    when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.appends).thenAnswer((_) => const Stream.empty());
    when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.load()).thenAnswer((_) async => loadOf());
    when(() => repo.channels()).thenAnswer((_) async => _channels);
    when(() => repo.boardMembers()).thenAnswer((_) async => const [_ada, _lena]);
    when(() => repo.groupChat(channel: any(named: 'channel'))).thenAnswer((i) async {
      final channel = i.namedArguments[#channel] as String?;
      return snapshotOf(channel ?? 'c-general', messages: messages[channel] ?? const []);
    });
    when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
        .thenAnswer((_) async => receiptOf(const CommandResultGroupMessageSent(conversationId: 'c-atlas', messageId: 'm-new', runIds: [], to: [])));
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
      GoRoute(
        path: '/board/new',
        builder: (_, s) {
          final d = s.extra as TicketDraft?;
          return Scaffold(body: Text('new ticket: ${d?.title} | ${d?.projectId} | ${d?.fromMessage}'));
        },
      ),
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

  /// Opens [channel] as a link does (ticket #71: no Team tab, no picker).
  Future<void> pick(WidgetTester tester, String channel) async {
    await tester.tap(find.byKey(ValueKey('channel-$channel')));
    await board.settleBoard(tester);
  }

  group('channels (MU2-08)', () {
    testWidgets("a project's channel is read and written with its channel (AC3)", (tester) async {
      await pump(tester, messages: {
        'c-atlas': [messageOf('a1', MessageRole.assistant, 'The vault design is ready.', update: 'group', author: 'Orion')],
      });
      await pick(tester, 'c-atlas');
      expect(find.text('#atlas'), findsOneWidget);
      expect(find.text('The vault design is ready.'), findsOneWidget);
      verify(() => repo.groupChat(channel: 'c-atlas')).called(greaterThan(0));
      await tester.enterText(find.byType(TextField).last, 'Can we add an audit export?');
      await tester.pump();
      await tester.tap(find.byKey(const Key('composer-send')));
      await board.settleBoard(tester);
      final send = writesOf(repo).single;
      expect(send['method'], 'group_chat.send');
      expect(paramsOf(send)['channel'], 'c-atlas');
    });

    testWidgets("an archived project's channel can only be read (AC2)", (tester) async {
      await pump(tester);
      await pick(tester, 'c-old');
      expect(find.text('This project is archived · read-only.'), findsOneWidget);
      expect(find.byKey(const Key('composer-send')), findsNothing);
    });

    test('a channel is labelled #name', () {
      expect(channelLabel(channelOf('x', 'Segrusa Web', projectId: 'p')), '#segrusa-web');
    });

    testWidgets('a tab with a dot shows a small badge without a number', (tester) async {
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          bottomNavigationBar: GlassTabBar(current: 0, onTap: (_) {}, tabs: const [GlassTab(CupertinoIcons.house, 'Home'), GlassTab(CupertinoIcons.chat_bubble_2, 'Team', dot: true)]),
        ),
      ));
      final badges = tester.widgetList<Badge>(find.byType(Badge)).toList();
      expect(badges.map((b) => b.isLabelVisible), [false, true]);
      expect(badges.last.label, isNull);
    });
  });

  group('a ticket from a message (MU2-10)', () {
    testWidgets('long press → Create ticket: the form opens filled in, with the channel\'s project (AC1)', (tester) async {
      await pump(tester, messages: {
        'c-atlas': [postOf('m1', 'Add an audit export\nWith dates.', by: _lena)],
      });
      await pick(tester, 'c-atlas');
      expect(find.text('lena · board member'), findsOneWidget);
      await tester.longPress(find.textContaining('Add an audit export', findRichText: true).first);
      await board.settleBoard(tester);
      await tester.tap(find.text('Create ticket'));
      await board.settleBoard(tester);
      expect(find.text('new ticket: Add an audit export | p1 | m1'), findsOneWidget);
    });

    testWidgets("an employee's answer has Create ticket in its sheet; in General the project is left to choose", (tester) async {
      await pump(tester, messages: {
        null: [messageOf('a1', MessageRole.assistant, 'We should rotate the keys.', update: 'group', author: 'Orion')],
      });
      await pick(tester, 'c-general');
      await tester.longPress(find.text('We should rotate the keys.'));
      await board.settleBoard(tester);
      await tester.tap(find.byKey(const Key('message-create-ticket')));
      await board.settleBoard(tester);
      expect(find.text('new ticket: We should rotate the keys. | null | a1'), findsOneWidget);
    });

    testWidgets('the message links to its ticket (→ #52), and a second ticket from it asks first (AC3, §4)', (tester) async {
      await pump(tester, messages: {
        null: [
          postOf('m1', 'Ship the export', by: _lena),
          lineOf('s1', SystemLineKind.ticketCreated, 'ada created #52 from a message', ticketId: 'k52', messageId: 'm1'),
        ],
      });
      await pick(tester, 'c-general');
      expect(find.textContaining('ada created #52 from a message'), findsOneWidget);
      expect(find.text('→ #52'), findsOneWidget);
      await tester.longPress(find.text('Ship the export'));
      await board.settleBoard(tester);
      await tester.tap(find.text('Create ticket'));
      await board.settleBoard(tester);
      expect(find.text('#52 was already created from this message. Create another?'), findsOneWidget);
      await tester.tap(find.text('Create another'));
      await board.settleBoard(tester);
      expect(find.textContaining('new ticket: Ship the export'), findsOneWidget);
    });

    testWidgets('the → #52 pill opens the ticket', (tester) async {
      await pump(tester, messages: {
        null: [
          postOf('m1', 'Ship the export', by: _lena),
          lineOf('s1', SystemLineKind.ticketCreated, 'ada created #52 from a message', ticketId: 'k52', messageId: 'm1'),
        ],
      });
      await pick(tester, 'c-general');
      await tester.tap(find.text('→ #52'));
      await board.settleBoard(tester);
      expect(find.text('ticket k52'), findsOneWidget);
    });

    Future<board.MockRepository> form(WidgetTester tester, LinkState link) async {
      final repo = board.boardRepository();
      when(() => repo.linkState).thenReturn(link);
      when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')))
          .thenAnswer((_) async => receiptOf(const CommandResultTicketCreated(number: 52, ticketId: 'k52')));
      const draft = TicketDraft(title: 'Add an audit export', description: '> Add an audit export\n\n*From: Team chat · #atlas*', projectId: 'p1', fromMessage: 'm1');
      await board.pumpBoardPage(tester, repo, const NewTicketPage(draft: draft), link: link);
      return repo;
    }

    testWidgets('Create sends from_message and lands in Backlog; nothing starts (AC1/AC2)', (tester) async {
      final repo = await form(tester, linkWith());
      expect(find.text('Add an audit export'), findsWidgets);
      await tester.tap(find.byKey(const ValueKey('new-ticket-create')));
      await board.settleBoard(tester);
      final create = writesOf(repo).single;
      expect(create['method'], 'ticket.create');
      expect(paramsOf(create)['from_message'], 'm1');
      expect(paramsOf(create)['project_id'], 'p1');
      expect(paramsOf(create)['description'], '> Add an audit export\n\n*From: Team chat · #atlas*');
      expect(paramsOf(create).containsKey('start'), isFalse);
      expect(find.text('#52 created in Backlog.'), findsOneWidget);
    });

    testWidgets('a server without ticket_discussion gets no from_message (it refuses unknown fields)', (tester) async {
      final repo = await form(tester, board.onlineLink);
      await tester.tap(find.byKey(const ValueKey('new-ticket-create')));
      await board.settleBoard(tester);
      expect(paramsOf(writesOf(repo).single).containsKey('from_message'), isFalse);
    });
  });
}
