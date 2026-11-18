import 'dart:async';

import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/discussion.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/board/ticket_page.dart';
import 'package:aurelius_mobile/features/chat/chat_bloc.dart';
import 'package:aurelius_mobile/features/chat/chat_view.dart' show isSystemLine;
import 'package:aurelius_mobile/features/company/company_bloc.dart';
import 'package:aurelius_mobile/features/connection/connection_bloc.dart';
import 'package:aurelius_mobile/features/inbox/inbox_bloc.dart';
import 'package:aurelius_mobile/features/runtime/runtime_bloc.dart';
import 'package:aurelius_mobile/features/team/channels_bloc.dart';
import 'package:aurelius_mobile/features/team/team_page.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/phase2_wire.dart';
import 'support/tickets.dart';

// Phase 2 against the engine as it is (AUR-MU-02-M2): the server writes the
// sentence of every system line, puts a decision's feedback in the discussion
// itself, and counts unread and mentions per board member (reading marks read).
// The fixtures follow the server's JSON (support/phase2_wire.dart).

const _phase2 = LinkState(
  status: LinkStatus.online,
  capabilities: ['read', 'chat', 'task_control', 'approve'],
  features: [ticketDiscussionFeature, teamChannelsFeature],
  boardMembersExist: true,
  user: testUser,
);

const _lena = UserView(id: lenaId, username: 'lena', createdAtMs: 0);

TicketView _ticket(ColumnKind column) => TicketView.fromJson({
      ...ticketOf(column: column, ownerEmployeeId: 'forge', ownerName: 'Forge', rootTaskId: 'root').toJson(),
      'conversation_id': 'disc-1',
    });

Event _posted(String conversation, {int seq = 20}) => Event(aggregateId: conversation, atMs: 0, kind: EventKind.messageUserCommitted, seq: seq);

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  group('the wire shape', () {
    test('a discussion decodes: system lines carry their sentence, the feedback is the board member\'s own message', () {
      final messages = messagesResponse(discussionId, discussionLife()).messages;
      final lines = [for (final m in messages) if (m.system != null) (m.system!.kind, m.text)];
      expect(lines, [
        (SystemLineKind.moved, 'ada moved Backlog → Todo'),
        (SystemLineKind.plan, 'Forge handed in a plan · waiting for a board member'),
        (SystemLineKind.delegated, 'Forge delegated a task to Schema'),
        (SystemLineKind.review, 'Forge handed in the report · Review'),
        (SystemLineKind.done, 'ada put it on Done'),
        (SystemLineKind.summaryFailed, 'The summary for project memory failed'),
      ]);
      final feedback = messages.singleWhere((m) => m.text == 'Smaller steps');
      expect((feedback.isUser, authorOf(feedback), feedback.system), (true, 'ada', null));
      expect(messages.where((m) => m.system != null).every((m) => isSystemLine(m, inGroup: true) && m.author == 'Aurelius'), isTrue);
    });

    test('ticket_created names the source message and the number is read from the server\'s sentence (MU2-10 AC3)', () {
      final made = ticketsFromMessages(messagesResponse(orbitChannelId, channelWithTicket()).messages);
      expect(made.keys, ['01900000-0000-7000-8000-000000000201']);
      expect((made.values.single.ticketId, made.values.single.label), (madeTicketId, '#12'));
      expect(ticketNumberIn('team#3 created #12 from a message'), 12, reason: 'a name with a # of its own');
      expect(ticketNumberIn('ada created a ticket from a message'), isNull, reason: 'a ticket the server no longer finds');
    });

    test('channel.list decodes with unread and mentions per board member; the badge counts mentions (B8)', () {
      final channels = channelsResponse([
        channelJson(generalId, 'General', unread: 2),
        channelJson(orbitChannelId, 'Orbit', projectId: orbitProjectId, unread: 3, mentions: 1),
      ]);
      expect(channels.map((c) => (c.name, c.unread, c.mentions)), [('General', 2, 0), ('Orbit', 3, 1)]);
      expect(teamBadge(channels), (mentions: 1, unread: true));
      final read = channelsResponse([channelJson(generalId, 'General'), channelJson(orbitChannelId, 'Orbit', projectId: orbitProjectId)]);
      expect(teamBadge(read), (mentions: 0, unread: false));
    });
  });

  group('a ticket\'s discussion from the server', () {
    late MockRepository repo;
    late StreamController<Event> changes;
    late List<Map<String, dynamic>> wire;

    Future<void> open(WidgetTester tester, ColumnKind column, List<Map<String, dynamic>> messages, {Size size = const Size(390, 844)}) async {
      wire = messages;
      changes = StreamController<Event>.broadcast();
      addTearDown(changes.close);
      final ticket = _ticket(column);
      repo = boardRepository(detail: TicketDetail(asOfSeq: 1, ticket: ticket, tasks: const []));
      when(() => repo.messages(any())).thenAnswer((_) async => messagesResponse('disc-1', wire));
      when(() => repo.chatChanges).thenAnswer((_) => changes.stream);
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => loadOf());
      when(() => repo.boardMembers()).thenAnswer((_) async => const [testUser, _lena]);
      await pumpBoardPage(tester, repo, TicketPage(ticketId: ticket.id, discussion: true), link: _phase2, size: size);
    }

    testWidgets('the server\'s sentences show as they are, the feedback once as ada\'s message', (tester) async {
      // Tall enough for the whole discussion: the list builds what is on screen.
      await open(tester, ColumnKind.done, discussionLife(), size: const Size(390, 3200));
      for (final sentence in ['ada moved Backlog → Todo', 'Forge delegated a task to Schema', 'ada put it on Done']) {
        expect(find.text(sentence), findsOneWidget, reason: sentence);
      }
      expect(find.text('Forge handed in a plan · waiting for a board member'), findsOneWidget, reason: 'the plan card');
      expect(find.text('Forge handed in the report · Review'), findsOneWidget, reason: 'the report card');
      expect(find.text('Smaller steps'), findsOneWidget);
      expect(find.text('ada · board member'), findsOneWidget, reason: 'the feedback is under its author');
      expect(find.text('The table is in.'), findsOneWidget, reason: 'an employee\'s copied answer');
    });

    testWidgets('Approve, accept edits with text in the input: one ticket.approve_plan with the feedback, no ticket.post; the server\'s copy shows once (MU2-05 AC3)', (tester) async {
      final upToPlan = discussionLife().take(3).toList();
      await open(tester, ColumnKind.planReview, upToPlan);
      await tester.enterText(find.byType(TextField).last, 'Smaller steps');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('gate-approve-edits')));
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.approve_plan');
      expect(paramsOf(write), {'ticket_id': 'k1', 'decision': 'approve_edits', 'feedback': 'Smaller steps'});
      expect(find.text('Smaller steps'), findsNothing, reason: 'the composer is empty and the app shows no copy of its own');
      // The engine posts it as ada's message; the event brings it.
      wire = [...upToPlan, memberPost('01900000-0000-7000-8000-000000000104', 'Smaller steps', userId: adaId, username: 'ada')];
      changes.add(_posted('disc-1'));
      await settleBoard(tester);
      expect(find.text('Smaller steps'), findsOneWidget);
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
    });

    testWidgets('Send back with text: one ticket.send_back, nothing posted by the app (MU2-06)', (tester) async {
      await open(tester, ColumnKind.review, discussionLife().take(7).toList());
      await tester.enterText(find.byType(TextField).last, 'Add a test for the cost.');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('bar-send-back')));
      await settleBoard(tester);
      expect(writesOf(repo).map((w) => w['method']), ['ticket.send_back']);
    });

    testWidgets('a failed summary: Try again sends ticket.retry_summary for the line\'s ticket (MU2-12 AC4)', (tester) async {
      await open(tester, ColumnKind.done, discussionLife());
      expect(find.text('⚠ The summary for project memory failed'), findsOneWidget);
      await tester.tap(find.byKey(const Key('system-line-action')));
      await settleBoard(tester);
      final write = writesOf(repo).single;
      expect(write['method'], 'ticket.retry_summary');
      expect(paramsOf(write), {'ticket_id': ticketId});
    });

    testWidgets('failed again after Try again: only the last line has Try again', (tester) async {
      await open(tester, ColumnKind.done, [...discussionLife(), summaryLine('01900000-0000-7000-8000-000000000110', saved: false)]);
      expect(find.text('⚠ The summary for project memory failed'), findsNWidgets(2));
      expect(find.byKey(const Key('system-line-action')), findsOneWidget);
    });

    testWidgets('saved after a failure: Try again is gone, the line says so (MU2-12 AC2)', (tester) async {
      await open(tester, ColumnKind.done, [...discussionLife(), summaryLine('01900000-0000-7000-8000-000000000110', saved: true)]);
      expect(find.text('Summary saved to project memory'), findsOneWidget);
      expect(find.byKey(const Key('system-line-action')), findsNothing);
    });
  });

  group('the Team chat reads a channel only on screen', () {
    late MockRepository repo;
    late StreamController<Event> changes;

    void stub() {
      repo = MockRepository();
      changes = StreamController<Event>.broadcast();
      when(() => repo.linkState).thenReturn(_phase2);
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.chatChanges).thenAnswer((_) => changes.stream);
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.boardMembers()).thenAnswer((_) async => const [testUser, _lena]);
      when(() => repo.groupChat(channel: any(named: 'channel'))).thenAnswer((_) async => messagesResponse(orbitChannelId, channelWithTicket()));
    }

    Future<void> settle() => Future<void>.delayed(const Duration(milliseconds: 200));

    test('hidden, a change waits; shown again it reads once and says so (schema 20 marks read)', () async {
      stub();
      addTearDown(changes.close);
      var reads = 0;
      final bloc = ChatBloc.group(repo, channel: orbitChannelId, onRead: () => reads++);
      addTearDown(bloc.close);
      await settle();
      expect(reads, 1);
      bloc.add(const ChatShown(false));
      await settle();
      changes.add(_posted(orbitChannelId));
      changes.add(_posted(orbitChannelId, seq: 21));
      await settle();
      verify(() => repo.groupChat(channel: orbitChannelId)).called(1);
      expect(reads, 1, reason: 'nothing read while hidden: it stays unread on the server');
      bloc.add(const ChatShown(true));
      await settle();
      verify(() => repo.groupChat(channel: orbitChannelId)).called(1);
      expect(reads, 2);
      bloc.add(const ChatShown(true));
      await settle();
      verifyNever(() => repo.groupChat(channel: orbitChannelId));
    });

    test('a ticket\'s discussion still reads while hidden (its dot is counted on the phone)', () async {
      stub();
      addTearDown(changes.close);
      when(() => repo.messages(any())).thenAnswer((_) async => messagesResponse('disc-1', discussionLife()));
      final bloc = ChatBloc.discussion(repo, ticketId: 'k1', conversationId: 'disc-1');
      addTearDown(bloc.close);
      await settle();
      bloc.add(const ChatShown(false));
      changes.add(_posted('disc-1'));
      await settle();
      verify(() => repo.messages('disc-1')).called(2);
    });
  });

  group('the Team page with the server\'s channels', () {
    late MockRepository repo;
    late StreamController<Event> changes;
    late List<Map<String, dynamic>> Function(bool generalRead) counts;
    var generalRead = false;

    /// The Team page; [shown] is its tab's TickerMode (the shell turns it off when hidden).
    Future<ValueNotifier<bool>> pump(WidgetTester tester) async {
      tester.view.physicalSize = const Size(390, 844) * 3;
      tester.view.devicePixelRatio = 3;
      addTearDown(tester.view.reset);
      repo = MockRepository();
      changes = StreamController<Event>.broadcast();
      addTearDown(changes.close);
      when(() => repo.host).thenReturn(const Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0));
      when(() => repo.linkState).thenReturn(_phase2);
      when(() => repo.online).thenAnswer((_) => const Stream.empty());
      when(() => repo.changes).thenAnswer((_) => const Stream.empty());
      when(() => repo.chatChanges).thenAnswer((_) => changes.stream);
      when(() => repo.appends).thenAnswer((_) => const Stream.empty());
      when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
      when(() => repo.load()).thenAnswer((_) async => loadOf());
      when(() => repo.boardMembers()).thenAnswer((_) async => const [testUser, _lena]);
      // What the server counts for ada: reading General (group_chat.get) marks it read.
      generalRead = false;
      when(() => repo.channels()).thenAnswer((_) async => channelsResponse(counts(generalRead)));
      when(() => repo.groupChat(channel: any(named: 'channel'))).thenAnswer((i) async {
        final channel = i.namedArguments[#channel] as String?;
        if (channel == null) generalRead = true;
        return messagesResponse(channel ?? generalId, channel == orbitChannelId ? channelWithTicket() : [memberPost('01900000-0000-7000-8000-000000000301', 'Morning all.')]);
      });
      final company = MockCompanyBloc();
      whenListen(company, const Stream<CompanyState>.empty(), initialState: CompanyState(loading: false, employees: [orion, forge]));
      final inbox = MockInboxBloc();
      whenListen(inbox, const Stream<InboxState>.empty(), initialState: const InboxState(loading: false));
      final connection = MockConnectionBloc();
      whenListen(connection, const Stream<LinkState>.empty(), initialState: _phase2);
      final runtime = MockRuntimeBloc();
      whenListen(runtime, const Stream<RuntimeState>.empty(), initialState: const RuntimeState());
      final shown = ValueNotifier(true);
      addTearDown(shown.dispose);
      final router = GoRouter(initialLocation: '/', routes: [
        GoRoute(path: '/', builder: (_, _) => ValueListenableBuilder<bool>(valueListenable: shown, builder: (_, on, child) => TickerMode(enabled: on, child: child!), child: const TeamPage())),
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
      await settleBoard(tester);
      return shown;
    }

    Future<void> openOrbit(WidgetTester tester) async {
      await tester.tap(find.byKey(const ValueKey('channel-picker')));
      await settleBoard(tester);
      await tester.tap(find.byKey(const ValueKey('channel-$orbitChannelId')));
      await settleBoard(tester);
    }

    testWidgets('after reading a channel the counts are asked again: the server\'s, no tally of the phone', (tester) async {
      counts = (read) => [channelJson(generalId, 'General', unread: read ? 0 : 2, mentions: read ? 0 : 1), channelJson(orbitChannelId, 'Orbit', projectId: orbitProjectId, unread: 3, mentions: 1)];
      await pump(tester);
      expect(find.text('● 3 unread'), findsOneWidget, reason: 'Orbit, by the server');
      verifyInOrder([() => repo.groupChat(channel: null), () => repo.channels()]);
      final bloc = tester.element(find.byType(TeamPage)).read<ChannelsBloc>();
      expect(bloc.state.byId(generalId)!.unread, 0, reason: 'General was read: the server says 0 now');
      expect(bloc.state.badge, (mentions: 1, unread: true));
    });

    testWidgets('a hidden Team tab does not read (and so does not mark read) what comes in', (tester) async {
      counts = (_) => [channelJson(generalId, 'General'), channelJson(orbitChannelId, 'Orbit', projectId: orbitProjectId)];
      final shown = await pump(tester);
      verify(() => repo.groupChat(channel: null)).called(1);
      shown.value = false;
      await settleBoard(tester);
      changes.add(_posted(generalId));
      await settleBoard(tester);
      verifyNever(() => repo.groupChat(channel: any(named: 'channel')));
      shown.value = true;
      await settleBoard(tester);
      verify(() => repo.groupChat(channel: null)).called(1);
    });

    testWidgets('the message a ticket was made from has "→ #12"; it opens that ticket (MU2-10 AC3)', (tester) async {
      counts = (_) => [channelJson(generalId, 'General'), channelJson(orbitChannelId, 'Orbit', projectId: orbitProjectId)];
      await pump(tester);
      await openOrbit(tester);
      expect(find.text('The export button does nothing on Safari.'), findsOneWidget);
      expect(find.text('ada created #12 from a message'), findsOneWidget, reason: 'the server\'s sentence');
      expect(find.byKey(const ValueKey('ticket-pill-$madeTicketId')), findsOneWidget);
      expect(find.text('→ #12'), findsOneWidget);
      await tester.tap(find.text('→ #12'));
      await settleBoard(tester);
      expect(find.text('ticket $madeTicketId'), findsOneWidget);
    });
  });
}
