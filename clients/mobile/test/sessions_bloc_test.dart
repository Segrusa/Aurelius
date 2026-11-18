import 'dart:async';

import 'package:aurelius_mobile/core/net/connection.dart' show LinkState, LinkStatus;
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/protocol/protocol.dart';
import 'package:aurelius_mobile/features/sessions/sessions_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import 'support/builders.dart';
import 'support/tickets.dart';

const sessionsLink = LinkState(status: LinkStatus.online, capabilities: ['read', 'chat'], features: ['work_sessions'], boardMembersExist: true, user: testUser, claudeLogin: testLogin);

WorkSessionView sessionOf(String id, {int at = 0, bool running = false, SessionDispatch? dispatch, String title = 'A session'}) => WorkSessionView(
      conversationId: id,
      createdAtMs: 0,
      employeeId: 'e1',
      employeeName: 'Forge',
      lastActivityMs: at,
      projectId: 'p1',
      projectName: 'Aurelius',
      running: running,
      title: title,
      dispatch: dispatch,
    );

Event chatEvent() => const Event(aggregateId: 'c1', atMs: 0, kind: EventKind.messageUserCommitted, seq: 1);

void main() {
  late MockRepository repo;
  late StreamController<Event> chat;
  late StreamController<LinkState> link;

  setUp(() {
    repo = MockRepository();
    chat = StreamController<Event>.broadcast();
    link = StreamController<LinkState>.broadcast();
    when(() => repo.chatChanges).thenAnswer((_) => chat.stream);
    when(() => repo.link).thenAnswer((_) => link.stream);
    when(() => repo.online).thenAnswer((_) => const Stream.empty());
  });

  tearDown(() async {
    await chat.close();
    await link.close();
  });

  test('a server without work_sessions reads nothing and says so, without an error', () async {
    when(() => repo.linkState).thenReturn(onlineLink);
    final bloc = SessionsBloc(repo);
    await pumpEventQueue();
    expect(bloc.state.supported, isFalse);
    expect(bloc.state.loading, isFalse);
    expect(bloc.state.error, isNull);
    verifyNever(() => repo.workSessions());
    await bloc.close();
  });

  test('reads work_session.list, newest activity first, and counts who works', () async {
    when(() => repo.linkState).thenReturn(sessionsLink);
    when(() => repo.workSessions()).thenAnswer((_) async => [sessionOf('a', at: 1, running: true), sessionOf('b', at: 3), sessionOf('c', at: 2, dispatch: SessionDispatch.nobody)]);
    final bloc = SessionsBloc(repo);
    final state = await stateWhere(bloc, (s) => !s.loading);
    expect(state.sessions.map((s) => s.conversationId), ['b', 'c', 'a']);
    expect(state.working, 1);
    await bloc.close();
  });

  test('a change while shown reads again; while hidden it only marks the list stale', () async {
    when(() => repo.linkState).thenReturn(sessionsLink);
    when(() => repo.workSessions()).thenAnswer((_) async => [sessionOf('a')]);
    final bloc = SessionsBloc(repo, debounce: Duration.zero);
    await stateWhere(bloc, (s) => !s.loading);
    verify(() => repo.workSessions()).called(1);

    chat.add(chatEvent());
    await pumpEventQueue();
    await Future<void>.delayed(const Duration(milliseconds: 10));
    verify(() => repo.workSessions()).called(1);

    bloc.add(const SessionsShown(false));
    await pumpEventQueue();
    chat.add(chatEvent());
    chat.add(chatEvent());
    await Future<void>.delayed(const Duration(milliseconds: 10));
    verifyNever(() => repo.workSessions());

    bloc.add(const SessionsShown(true));
    await Future<void>.delayed(const Duration(milliseconds: 10));
    verify(() => repo.workSessions()).called(1);
    await bloc.close();
  });

  test('a failed read shows why instead of loading on; Try again reads again', () async {
    when(() => repo.linkState).thenReturn(sessionsLink);
    when(() => repo.workSessions()).thenThrow(const ProtocolException(ErrorCode.internal, 'The server broke.'));
    final bloc = SessionsBloc(repo);
    final failed = await stateWhere(bloc, (s) => !s.loading);
    expect(failed.error, isNotNull);

    when(() => repo.workSessions()).thenAnswer((_) async => [sessionOf('a')]);
    bloc.add(const SessionsRefreshed());
    final ok = await stateWhere(bloc, (s) => s.sessions.isNotEmpty);
    expect(ok.error, isNull);
    await bloc.close();
  });

  test('the feature arriving later turns the list on', () async {
    var current = onlineLink;
    when(() => repo.linkState).thenAnswer((_) => current);
    when(() => repo.workSessions()).thenAnswer((_) async => [sessionOf('a')]);
    final bloc = SessionsBloc(repo);
    await pumpEventQueue();
    expect(bloc.state.supported, isFalse);
    current = sessionsLink;
    link.add(sessionsLink);
    final state = await stateWhere(bloc, (s) => s.supported && s.sessions.isNotEmpty);
    expect(state.sessions.single.conversationId, 'a');
    await bloc.close();
  });
}
