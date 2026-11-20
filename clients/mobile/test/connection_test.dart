import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/data/commands.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fake_daemon.dart';

void main() {
  late FakeDaemon daemon;
  late FakeApi api;
  late AureliusConnection connection;

  setUp(() async {
    daemon = FakeDaemon();
    api = FakeApi();
    connection = AureliusConnection(host: testHost, store: await storeWithSecret(), api: api, connect: daemon.connect);
  });

  tearDown(() => connection.dispose());

  test('connects: session, hello, health, subscribe → online with capabilities', () async {
    connection.start();
    await until(() => connection.state.isOnline);
    expect(connection.state.can('approve'), isTrue);
    expect(api.sessions, 1);
  });

  test('an unanswered write is resent with the same idempotency key after reconnect: applied once (R02)', () async {
    connection.start();
    await until(() => connection.state.isOnline);
    daemon.dropNextCommand = true;
    final receipt = connection.write(Commands.sendEmployeeMessage('mgr', 'Hello', conversationId: 'conv-1'));
    await until(() => !connection.state.isOnline);
    connection.wake();
    final result = await receipt;
    expect(result.replayed, isTrue, reason: 'the PC had applied it; the resend got the stored receipt');
    expect(daemon.executed, hasLength(1));
    expect(daemon.sockets.length, greaterThanOrEqualTo(2));
  });

  test('decisions are never replayed after a reconnect', () async {
    connection.start();
    await until(() => connection.state.isOnline);
    daemon.dropNextCommand = true;
    final decision = connection.write(Commands.decideApproval('a', 'digest', true), retry: false);
    await expectLater(decision, throwsA(isA<NotSentException>()));
  });

  test('offline: decisions fail at once, chat messages wait for the reconnect', () async {
    daemon.reachable = false;
    connection.start();
    await until(() => connection.state.status == LinkStatus.offline);
    await expectLater(connection.write(Commands.decideApproval('a', 'd', false), retry: false), throwsA(isA<NotSentException>()));
    final queued = connection.write(Commands.sendEmployeeMessage('mgr', 'Later', conversationId: 'conv-1'));
    daemon.reachable = true;
    connection.wake();
    expect((await queued).replayed, isFalse);
    expect(daemon.executed, hasLength(1));
  });

  test('a revoked device ends in revoked, not in endless retries (R04)', () async {
    connection.start();
    await until(() => connection.state.isOnline);
    api.revoked = true;
    await daemon.current!.drop(4401);
    await until(() => connection.state.status == LinkStatus.revoked);
    expect(connection.state.message, contains('Pair again'));
  });

  test('only the major is checked: an older server (4.x) is refused with "update"', () async {
    daemon.helloProtocol = 4;
    connection.start();
    await until(() => connection.state.status == LinkStatus.incompatible);
    expect(connection.state.message, 'Update Aurelius on the phone and the PC.');
  });

  test('a server that refuses our major (incompatible_protocol) says the same', () async {
    daemon.serverProtocol = 6;
    connection.start();
    await until(() => connection.state.status == LinkStatus.incompatible);
    expect(connection.state.message, 'Update Aurelius on the phone and the PC.');
    expect(daemon.sockets, hasLength(1), reason: 'no reconnect loop');
  });

  test('a host that switched environment is refused', () async {
    daemon.environment = 'dev';
    connection.start();
    await until(() => connection.state.status == LinkStatus.incompatible);
  });

  test('an offline host says so honestly (R07)', () async {
    api.unreachable = true;
    connection.start();
    await until(() => connection.state.status == LinkStatus.offline);
    expect(connection.state.message, contains('not reachable'));
    expect(connection.state.lastOnlineMs, isNull);
  });

  test('asks for activity changes and merges them into whole lists (activity_preview)', () async {
    connection.start();
    await until(() => connection.state.isOnline);
    // server_name: only then does server.renamed come (SN-02 AC3);
    // ticket_channels: only then does channel.list list ticket channels (AUR-CHAT-001);
    // chat_threads / chat_reactions: only then do thread streams and message.reacted come (AUR-CHAT-002).
    // ticket_follow: only then does ticket.follow_changed come (AUR-FA-M1).
    expect(daemon.hello?['accepts'], ['activity_changes', 'server_name', 'ticket_channels', 'chat_threads', 'chat_reactions', 'ticket_follow', 'message_edits']);
    final got = <List<String>?>[];
    final sub = connection.appends.listen((a) => got.add(a.activity?.map((c) => '${c.callId}:${c.ok}').toList()));
    addTearDown(sub.cancel);
    Map<String, dynamic> call(String id, {bool? ok}) => {'call_id': id, 'tool': 'Bash', 'input': {'command': 'ls'}, 'offset': 0, 'ok': ?ok};
    void append(List<Map<String, dynamic>>? activity, {bool changes = false}) => daemon.current!.push({
          'type': 'stream_append',
          'run_id': 'r',
          'conversation_id': 'c',
          'message_id': 'm',
          'offset': 0,
          'text': '',
          'activity': ?activity,
          if (changes) 'activity_changes': true,
        });
    append([call('a')]);
    append([call('a', ok: true), call('b')], changes: true);
    append(null, changes: true);
    // The server forgets what it sent on a run event: the next list is whole.
    daemon.current!.push({'type': 'event', 'seq': 11, 'kind': 'run.started', 'aggregate_id': 'r', 'at_ms': 0});
    append([call('c')], changes: true);
    await until(() => got.length == 4);
    expect(got, [
      ['a:null'],
      ['a:true', 'b:null'],
      null,
      ['c:null'],
    ]);
  });

  test("a thread's stream keeps its thread_parent when its activity is merged (AUR-CHAT-002)", () async {
    connection.start();
    await until(() => connection.state.isOnline);
    final got = <String?>[];
    final sub = connection.appends.listen((a) => got.add(a.threadParent));
    addTearDown(sub.cancel);
    void append({bool changes = false}) => daemon.current!.push({
          'type': 'stream_append',
          'run_id': 'r',
          'conversation_id': 'c',
          'message_id': 'm',
          'offset': 0,
          'text': '',
          'activity': [
            {'call_id': 'a', 'tool': 'Bash', 'input': {'command': 'ls'}, 'offset': 0},
          ],
          if (changes) 'activity_changes': true,
          'thread_parent': 'root-1',
        });
    append();
    append(changes: true);
    await until(() => got.length == 2);
    expect(got, ['root-1', 'root-1']);
  });

  test('an event kind this app does not know is dropped; the next one still comes (AUR-CHAT-002)', () async {
    connection.start();
    await until(() => connection.state.isOnline);
    final got = <String>[];
    final sub = connection.events.listen((e) => got.add(e.kind.wire));
    addTearDown(sub.cancel);
    daemon.current!.push({'type': 'event', 'seq': 11, 'kind': 'message.something_new', 'aggregate_id': 'c', 'at_ms': 0});
    daemon.current!.push({'type': 'event', 'seq': 12, 'kind': 'message.reacted', 'aggregate_id': 'c', 'at_ms': 0});
    await until(() => got.isNotEmpty);
    expect(got, ['message.reacted']);
    expect(connection.state.isOnline, isTrue);
  });
}
