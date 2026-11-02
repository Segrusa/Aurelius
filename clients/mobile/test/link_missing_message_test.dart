import 'dart:async';

import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
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

// AUR-FA-M2: "That message no longer exists." only when the server says so:
// not when the read failed, the phone is offline or the list lacks it.

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

  Future<_MockRepository> pump(WidgetTester tester, Widget page, {LinkState? link, List<MessageView>? messages, List<ChannelView> channels = const [_channel], ThreadSnapshot? thread, TicketDetail? detail, void Function(_MockRepository repo)? stub}) async {
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
    when(() => repo.messageLocation(any())).thenThrow(const ProtocolException(ErrorCode.notFound, 'This message does not exist.'));
    when(() => repo.boardMembers()).thenAnswer((_) async => const [_ada]);
    when(() => repo.groupChat(channel: any(named: 'channel'))).thenAnswer((_) async => snapshotOf('c-42', messages: messages ?? _many));
    when(() => repo.messages(any())).thenAnswer((_) async => snapshotOf('c-42', messages: messages ?? _many));
    if (thread != null) when(() => repo.thread(any())).thenAnswer((_) async => thread);
    if (detail != null) when(() => repo.ticket(any())).thenAnswer((_) async => detail);
    when(() => repo.timeline(correlationId: any(named: 'correlationId'), activity: any(named: 'activity'), limit: any(named: 'limit'))).thenAnswer((_) async => const TimelinePage(cards: []));
    stub?.call(repo);
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

  const gone = 'That message no longer exists.';
  const notShown = 'Could not show that message. Try again.';

  group('a message a link names but the read lacks', () {
    testWidgets('a failed read is not gone, and Try again still goes to the message', (tester) async {
      var fail = true;
      await pump(tester, const ChannelLinkPage(conversationId: 'c-42', message: 'm3'), stub: (repo) {
        when(() => repo.groupChat(channel: any(named: 'channel'))).thenAnswer((_) async {
          if (fail) throw const ProtocolException(ErrorCode.internal, 'the read failed');
          return snapshotOf('c-42', messages: _many);
        });
        when(() => repo.messageLocation(any())).thenAnswer((_) async => const MessageLocation(conversationId: 'c-42', messageId: 'm3'));
      });
      await settle(tester, frames: 8);
      expect(find.text(gone), findsNothing);
      expect(find.text(notShown), findsOneWidget);
      await settle(tester);
      fail = false;
      await tester.tap(find.text('Try again'));
      await untilMarked(tester, 'flash-m3');
      expect(find.byKey(const ValueKey('flash-m3')), findsOneWidget);
      await settle(tester);
    });

    testWidgets('offline it is not gone either', (tester) async {
      await pump(tester, const ChannelLinkPage(conversationId: 'c-42', message: 'm3'), messages: const [], stub: (repo) {
        when(() => repo.messageLocation(any())).thenThrow(const DisconnectedException('offline'));
      });
      await settle(tester, frames: 8);
      expect(find.text(gone), findsNothing);
      expect(find.text(notShown), findsOneWidget);
      await settle(tester);
    });

    testWidgets('a message the server still has but the list lacks is not gone', (tester) async {
      await pump(tester, const ChannelLinkPage(conversationId: 'c-42', message: 'old'), stub: (repo) {
        when(() => repo.messageLocation(any())).thenAnswer((_) async => const MessageLocation(conversationId: 'c-42', messageId: 'old'));
      });
      await settle(tester, frames: 8);
      expect(find.text(gone), findsNothing);
      expect(find.text(notShown), findsOneWidget);
      await settle(tester);
    });

    testWidgets('only the server saying not found makes it gone, and it says so once', (tester) async {
      final repo = await pump(tester, const ChannelLinkPage(conversationId: 'c-42', message: 'nope'));
      await settle(tester, frames: 8);
      expect(find.text(gone), findsOneWidget);
      verify(() => repo.messageLocation('nope')).called(1);
      await settle(tester);
    });
  });
}
