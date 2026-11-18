import 'dart:async';

import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/core/theme/theme.dart';
import 'package:aurelius_mobile/core/theme/tokens.dart';
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
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/channel_opener.dart';
import 'support/tickets.dart' as board;

// AUR-CHAT-001-M1: a channel of the Slack-style Team chat, with a server of
// `ticket_channels` (ui-ontwerp §2–§4, §8). Ticket #71 took the Team tab and
// its channel list away; a channel opens from a link, as [ChannelOpener] does here.
// Neutral names: board members ada (signed in) and lena; employees Orion and Forge.

class MockRepository extends Mock implements AureliusRepository {}

class MockInboxBloc extends MockBloc<InboxEvent, InboxState> implements InboxBloc {}

class MockConnectionBloc extends MockBloc<ConnectionEvent, LinkState> implements ConnectionBloc {}

class MockCompanyBloc extends MockBloc<CompanyEvent, CompanyState> implements CompanyBloc {}

class MockRuntimeBloc extends MockBloc<RuntimeEvent, RuntimeState> implements RuntimeBloc {}

const _ada = UserView(id: 'u-1', username: 'ada', createdAtMs: 0);
const _lena = UserView(id: 'u-2', username: 'lena', createdAtMs: 0);
const _host = Host(hostId: 'h', name: 'desktop', url: 'http://x', clientId: 'c', environment: 'dev', pairedAtMs: 0);

LinkState _link({bool tickets = true}) => LinkState(
      status: LinkStatus.online,
      capabilities: const ['read', 'chat', 'task_control'],
      features: [ticketDiscussionFeature, teamChannelsFeature, if (tickets) ticketChannelsFeature],
      boardMembersExist: true,
      user: _ada,
    );

ChannelView _channel(String id, String name, {String? projectId, ChannelKind? kind, int unread = 0, int mentions = 0, bool archived = false, int? number, ColumnKind? column, int? readThrough, int at = 0}) => ChannelView(
      archived: archived,
      conversationId: id,
      kind: kind,
      lastActivityMs: at,
      mentions: mentions,
      name: name,
      projectId: projectId,
      readThrough: readThrough,
      ticketColumn: column,
      ticketId: number == null ? null : 'k$number',
      ticketNumber: number,
      unread: unread,
    );

final _general = _channel('c-general', 'General', kind: ChannelKind.general);
final _atlas = _channel('c-atlas', 'Atlas', projectId: 'p1', kind: ChannelKind.project, unread: 3);
final _web = _channel('c-web', 'Web', projectId: 'p2', kind: ChannelKind.project, unread: 2, mentions: 1);
final _t42 = _channel('c-42', '#42-login-per-board-member', projectId: 'p1', kind: ChannelKind.ticket, number: 42, column: ColumnKind.review, unread: 5, readThrough: 2, at: 200);
final _t52 = _channel('c-52', '#52-audit-export', projectId: 'p1', kind: ChannelKind.ticket, number: 52, column: ColumnKind.doing, at: 300);
final _t47 = _channel('c-47', '#47-vault-rotation', projectId: 'p1', kind: ChannelKind.ticket, number: 47, column: ColumnKind.done, archived: true, at: 100);

/// A time today, [minutes] after the start of this test's hour window (never yesterday).
int _today(int minutes) {
  final now = DateTime.now();
  final start = now.hour == 0 && now.minute < 30 ? DateUtils.dateOnly(now) : now.subtract(const Duration(minutes: 30));
  return start.add(Duration(minutes: minutes)).millisecondsSinceEpoch;
}

MessageView _msg(String id, String text, {UserView? by, String? author, required int at, int ordinal = 0, SystemLine? system, AgentMode? mode}) => MessageView(
      activity: const [],
      author: system != null ? 'Aurelius' : author,
      authorUser: by,
      createdAtMs: at,
      files: const [],
      id: id,
      mode: mode,
      ordinal: ordinal,
      queued: false,
      role: by != null ? MessageRole.user : MessageRole.assistant,
      runId: by != null || system != null ? null : 'run-$id',
      sources: const [],
      status: MessageStatus.complete,
      system: system,
      text: text,
      update: by != null || system != null ? null : 'group',
      uploads: const [],
    );

void main() {
  setUpAll(() => registerFallbackValue(<String, dynamic>{}));

  late MockRepository repo;
  late List<ChannelView> channels;

  Future<void> pump(
    WidgetTester tester, {
    bool tickets = true,
    List<ChannelView>? list,
    Map<String?, List<MessageView>> messages = const {},
    Size size = const Size(390, 844),
    Object? sendError,
    Stream<Event> changes = const Stream.empty(),
  }) async {
    tester.view.physicalSize = size * 3;
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.reset);
    final link = _link(tickets: tickets);
    channels = list ?? [_general, _atlas, _web, _t42, _t52, _t47];
    repo = MockRepository();
    when(() => repo.host).thenReturn(_host);
    when(() => repo.linkState).thenReturn(link);
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
    when(() => repo.changes).thenAnswer((_) => changes);
    when(() => repo.chatChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.boardChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.appends).thenAnswer((_) => const Stream.empty());
    when(() => repo.loadChanges).thenAnswer((_) => const Stream.empty());
    when(() => repo.load()).thenAnswer((_) async => loadOf());
    when(() => repo.channels()).thenAnswer((_) async => channels);
    when(() => repo.boardMembers()).thenAnswer((_) async => const [_ada, _lena]);
    when(() => repo.ticket(any())).thenAnswer((i) async => TicketDetail(asOfSeq: 1, ticket: board.ticketOf(id: i.positionalArguments.first as String, column: ColumnKind.done), tasks: const []));
    when(() => repo.ticket('k42')).thenAnswer((_) async => TicketDetail(
          asOfSeq: 1,
          ticket: board.ticketOf(id: 'k42', number: 42, title: 'Login per board member', column: ColumnKind.review, ownerName: 'Forge', ownerEmployeeId: 'forge'),
          tasks: [taskOf(id: 't1', employeeName: 'Forge'), taskOf(id: 't2', employeeName: 'Schema')],
        ));
    when(() => repo.groupChat(channel: any(named: 'channel'))).thenAnswer((i) async {
      final channel = i.namedArguments[#channel] as String?;
      return snapshotOf(channel ?? 'c-general', messages: messages[channel] ?? const []);
    });
    when(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).thenAnswer((_) async {
      if (sendError != null) throw sendError;
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

  double top(WidgetTester tester, String key) => tester.getTopLeft(find.byKey(ValueKey(key))).dy;

  ChannelsBloc channelsOf(WidgetTester tester) => ChannelOpener.channelsOf(tester);

  Future<void> open(WidgetTester tester, String id) async {
    await tester.tap(find.byKey(ValueKey('channel-$id')));
    await board.settleBoard(tester);
  }

  group('a ticket channel (§2, §3)', () {
    final messages = {
      'c-42': [
        _msg('m1', 'Please keep the argon2 params configurable.', by: _lena, at: _today(0), ordinal: 1),
        _msg('m2', 'Also add a test for the defaults.', by: _lena, at: _today(1), ordinal: 2),
        _msg('s1', 'Forge moved #42 Doing → Review', at: _today(2), ordinal: 3, system: const SystemLine(kind: SystemLineKind.moved, by: 'Forge')),
        _msg('m3', 'Done. @ada the cost is in config, @Orion knows.', author: 'Forge', at: _today(3), ordinal: 4),
        _msg('s2', 'The test run failed', at: _today(4), ordinal: 5, system: const SystemLine(kind: SystemLineKind.reported, action: 'tested_failed')),
      ],
    };

    testWidgets('the header shows the name, status and assignees, and Open goes to the ticket', (tester) async {
      await pump(tester, messages: messages);
      await open(tester, 'c-42');
      expect(find.byWidgetPredicate((w) => w is Text && w.key == const ValueKey('channel-title') && w.data == '#42-login-per-board-member'), findsOneWidget);
      final line = find.byKey(const ValueKey('channel-ticket-line'));
      expect(find.descendant(of: line, matching: find.text('Review')), findsOneWidget);
      expect(find.descendant(of: line, matching: find.text('Forge, Schema')), findsOneWidget, reason: 'the owner first, then the other employees of its tasks');
      verify(() => repo.ticket('k42')).called(greaterThan(0));
      await tester.tap(find.byKey(const ValueKey('channel-open-ticket')));
      await board.settleBoard(tester);
      expect(find.text('ticket k42'), findsOneWidget);
    });

    testWidgets('rows: one header for a group, a day separator, New messages after read_through, compact system lines', (tester) async {
      await pump(tester, messages: messages);
      await open(tester, 'c-42');
      expect(find.text('TODAY'), findsOneWidget);
      expect(find.text('lena · board member', findRichText: true), findsOneWidget, reason: "lena's two messages within five minutes share one header");
      expect(find.text('Please keep the argon2 params configurable.'), findsOneWidget);
      expect(find.text('New messages'), findsOneWidget);
      expect(top(tester, 'channel-row-m2') < tester.getTopLeft(find.text('New messages')).dy, isTrue, reason: 'read_through 2: the line goes after m2');
      expect(find.textContaining('→ Forge moved #42 Doing → Review · '), findsOneWidget);
      final failed = tester.widget<Text>(find.textContaining('✕ The test run failed · '));
      expect(failed.style?.color, Tokens.negative);
    });

    testWidgets('New messages stays where it was after opening marked the channel read', (tester) async {
      await pump(tester, messages: messages);
      // The server marks it read: the list's read_through moves on.
      channels = [_general, _channel('c-42', '#42-login-per-board-member', projectId: 'p1', kind: ChannelKind.ticket, number: 42, column: ColumnKind.review, readThrough: 5)];
      await open(tester, 'c-42');
      await board.settleBoard(tester);
      expect(find.text('New messages'), findsOneWidget);
    });

    testWidgets("a message naming the signed-in member is marked stronger than another's mention", (tester) async {
      await pump(tester, messages: messages);
      await open(tester, 'c-42');
      expect(find.text('@ you'), findsOneWidget);
      expect(find.bySemanticsLabel(RegExp('Forge, mentions you')), findsOneWidget);
      TextSpan? span(String text) {
        for (final rich in tester.widgetList<RichText>(find.byType(RichText))) {
          TextSpan? found;
          rich.text.visitChildren((s) {
            if (s is TextSpan && s.text == text) found = s;
            return found == null;
          });
          if (found != null) return found;
        }
        return null;
      }

      expect(span('@ada')?.style?.backgroundColor, Tokens.accentHalo);
      expect(span('@ada')?.style?.fontWeight, FontWeight.w600);
      expect(span('@Orion')?.style?.color, Tokens.accentText);
      expect(span('@Orion')?.style?.backgroundColor, isNull);
    });
  });

  group('a line in General opens its ticket (Q1-B1; ticket #71: no ticket channels on the phone)', () {
    Map<String?, List<MessageView>> general({String channel = 'c-42'}) => {
          null: [
            _msg('s1', 'lena opened #42 "Login per board member"', at: _today(0), ordinal: 1, system: SystemLine(kind: SystemLineKind.ticketCreated, by: 'lena', ticketId: 'k42', channelId: channel)),
            _msg('s2', 'ada closed #47 "Vault rotation"', at: _today(1), ordinal: 2, system: const SystemLine(kind: SystemLineKind.done, by: 'ada', ticketId: 'k47', channelId: 'c-47')),
          ],
        };

    testWidgets('a tap on an opened line opens the ticket page, also for a channel the list knows', (tester) async {
      await pump(tester, messages: general());
      await open(tester, 'c-general');
      await tester.tap(find.textContaining('lena opened #42'));
      await board.settleBoard(tester);
      expect(find.text('ticket k42'), findsOneWidget);
    });
  });

  group('a ticket channel updates live (Q1-B2)', () {
    testWidgets("a line or an employee's post in the open ticket's discussion reads the channel again and shows", (tester) async {
      final changes = StreamController<Event>.broadcast();
      addTearDown(changes.close);
      final messages = {
        'c-42': [_msg('m1', 'Please keep the argon2 params configurable.', by: _lena, at: _today(0), ordinal: 1)],
      };
      await pump(tester, messages: messages, changes: changes.stream);
      await open(tester, 'c-42');
      clearInteractions(repo);
      messages['c-42'] = [...messages['c-42']!, _msg('m2', 'Plan handed in.', author: 'Forge', at: _today(1), ordinal: 2)];
      // Discussion posts come as task.posted on the conversation, not as a chat event.
      changes.add(Event(aggregateId: 'c-42', atMs: 0, kind: EventKind.taskPosted, seq: 20));
      await board.settleBoard(tester);
      verify(() => repo.groupChat(channel: 'c-42')).called(1);
      expect(find.text('Plan handed in.'), findsOneWidget);
      // Another channel's post does not read this one again.
      changes.add(Event(aggregateId: 'c-52', atMs: 0, kind: EventKind.taskPosted, seq: 21));
      await board.settleBoard(tester);
      verifyNever(() => repo.groupChat(channel: 'c-42'));
    });

    testWidgets("the ticket's own event reads the channel again", (tester) async {
      final changes = StreamController<Event>.broadcast();
      addTearDown(changes.close);
      final messages = {'c-42': <MessageView>[]};
      await pump(tester, messages: messages, changes: changes.stream);
      await open(tester, 'c-42');
      clearInteractions(repo);
      messages['c-42'] = [_msg('s1', 'Forge moved #42 Review → Testing', at: _today(1), ordinal: 1, system: const SystemLine(kind: SystemLineKind.moved, by: 'Forge'))];
      changes.add(Event(aggregateId: 'k42', atMs: 0, kind: EventKind.ticketMoved, seq: 20));
      await board.settleBoard(tester);
      verify(() => repo.groupChat(channel: 'c-42')).called(1);
      expect(find.textContaining('Forge moved #42 Review → Testing'), findsOneWidget);
    });

    testWidgets('the channel turning read-only reads it again: the closed lines show under the read-only bar', (tester) async {
      final messages = {'c-42': <MessageView>[]};
      await pump(tester, messages: messages);
      await open(tester, 'c-42');
      messages['c-42'] = [_msg('s1', '#42 is closed · Login per board member', at: _today(1), ordinal: 1, system: const SystemLine(kind: SystemLineKind.done, by: 'ada'))];
      final bloc = channelsOf(tester);
      channels = [_general, _channel('c-42', '#42-login-per-board-member', projectId: 'p1', kind: ChannelKind.ticket, number: 42, column: ColumnKind.done, archived: true)];
      bloc.add(const ChannelsRefreshed());
      await board.settleBoard(tester);
      expect(find.text('#42 is Done · read-only.'), findsOneWidget);
      expect(find.textContaining('#42 is closed · Login per board member'), findsOneWidget);
    });
  });

  group('grouped rows', () {
    testWidgets("an employee's mode shows in the group's header row only, not above every grouped message", (tester) async {
      // Read through all three: no New messages line breaks the group.
      await pump(tester, list: [_general, _channel('c-42', '#42-login-per-board-member', projectId: 'p1', kind: ChannelKind.ticket, number: 42, column: ColumnKind.review, readThrough: 3)], messages: {
        'c-42': [
          _msg('m1', 'Looking at it.', author: 'Forge', at: _today(0), ordinal: 1, mode: AgentMode.ask),
          _msg('m2', 'Found the cause.', author: 'Forge', at: _today(1), ordinal: 2, mode: AgentMode.ask),
          _msg('m3', 'Fixed.', author: 'Forge', at: _today(2), ordinal: 3, mode: AgentMode.ask),
        ],
      });
      await open(tester, 'c-42');
      expect(find.text('Fixed.'), findsOneWidget);
      expect(find.textContaining('Ask mode'), findsOneWidget);
      expect(tester.getTopLeft(find.textContaining('Ask mode')).dy < top(tester, 'channel-row-m2'), isTrue, reason: 'in the first row');
    });
  });

  group('the composer (§4)', () {
    testWidgets('a hardware Enter sends, Shift+Enter does not; the message goes to the channel', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      await tester.enterText(find.byType(TextField).last, 'Looks good');
      await tester.pump();
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await board.settleBoard(tester);
      // Shift+Enter is a new line.
      verifyNever(() => repo.write(any(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry')));
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await board.settleBoard(tester);
      final send = writesOf(repo).single;
      expect(send['method'], 'group_chat.send');
      expect(paramsOf(send)['channel'], 'c-42');
      expect(paramsOf(send)['text'], 'Looks good');
    });

    testWidgets('the hint names the channel, and @ suggests who to mention', (tester) async {
      await pump(tester);
      await open(tester, 'c-42');
      expect(find.text('Message #42-login-per-board-member · @ to mention'), findsOneWidget);
      await tester.enterText(find.byType(TextField).last, 'Ask @Fo');
      await tester.pump();
      await tester.tap(find.byKey(const Key('mention-Forge')));
      await tester.pump();
      expect(tester.widget<TextField>(find.byType(TextField).last).controller!.text, 'Ask @Forge ');
    });
  });

  group('read-only and reopen (§3, §8)', () {
    testWidgets('a Done ticket shows the read-only bar with Open ticket instead of the composer', (tester) async {
      await pump(tester);
      await open(tester, 'c-47');
      expect(find.text('#47 is Done · read-only.'), findsOneWidget);
      expect(find.byKey(const Key('composer-send')), findsNothing);
      await tester.tap(find.byKey(const ValueKey('channel-read-only-open')));
      await board.settleBoard(tester);
      expect(find.text('ticket k47'), findsOneWidget);
    });

    testWidgets('channel_archived on a send shows the read-only bar, never the error; a reopen brings the composer back', (tester) async {
      await pump(tester, sendError: const ProtocolException(ErrorCode.channelArchived, 'channel_archived: the ticket is on Done'));
      await open(tester, 'c-42');
      await tester.enterText(find.byType(TextField).last, 'One more thing');
      await tester.pump();
      await tester.tap(find.byKey(const Key('composer-send')));
      await board.settleBoard(tester);
      expect(find.byKey(const Key('chat-read-only')), findsOneWidget);
      expect(find.byKey(const Key('composer-send')), findsNothing);
      expect(find.textContaining('channel_archived'), findsNothing, reason: 'never the raw error');
      expect(find.byType(SnackBar), findsNothing);
      // The server closed it: the list says archived; then the ticket leaves Done.
      final bloc = channelsOf(tester);
      channels = [_general, _channel('c-42', '#42-login-per-board-member', projectId: 'p1', kind: ChannelKind.ticket, number: 42, column: ColumnKind.done, archived: true)];
      bloc.add(const ChannelsRefreshed());
      await board.settleBoard(tester);
      expect(find.text('#42 is Done · read-only.'), findsOneWidget);
      channels = [_general, _channel('c-42', '#42-login-per-board-member', projectId: 'p1', kind: ChannelKind.ticket, number: 42, column: ColumnKind.doing)];
      bloc.add(const ChannelsRefreshed());
      await board.settleBoard(tester);
      expect(find.byKey(const Key('chat-read-only')), findsNothing);
      expect(find.byKey(const Key('composer-send')), findsOneWidget);
    });
  });
}
