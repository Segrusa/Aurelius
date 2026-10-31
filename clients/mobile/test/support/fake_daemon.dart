import 'dart:async';
import 'dart:convert';

import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/hosts/host_store.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/gateway_api.dart';

final testHost = Host(
  hostId: 'host-1',
  name: 'my-pc',
  url: 'https://my-pc.tail1.ts.net:8443',
  clientId: 'client-1',
  environment: 'production',
  pairedAtMs: 0,
);

/// A gateway that hands out sessions until revoked.
class FakeApi extends GatewayApi {
  bool revoked = false;
  bool unreachable = false;
  int sessions = 0;

  @override
  Future<Session> session(Host host, String deviceSecret) async {
    if (unreachable) throw const HostUnreachableException();
    if (revoked) throw const RevokedException();
    sessions++;
    return Session(token: 'token-$sessions', expiresAtMs: DateTime.now().add(const Duration(hours: 12)).millisecondsSinceEpoch, capabilities: const []);
  }
}

class FakeSocket implements LinkSocket {
  FakeSocket(this.daemon);

  final FakeDaemon daemon;
  final _incoming = StreamController<dynamic>();
  int? _closeCode;

  /// This connection's active board member (the daemon's `Peer.user`).
  String? user;
  int tries = 0;

  @override
  Stream<dynamic> get stream => _incoming.stream;

  @override
  int? get closeCode => _closeCode;

  @override
  void send(String text) {
    daemon.frames.add(text);
    daemon._handle(this, jsonDecode(text) as Map<String, dynamic>);
  }

  void push(Map<String, dynamic> message) {
    if (!_incoming.isClosed) _incoming.add(jsonEncode(message));
  }

  /// The PC drops the connection (optionally with a close code).
  Future<void> drop([int? code]) async {
    _closeCode = code;
    await _incoming.close();
  }

  @override
  Future<void> close() => drop();
}

/// Just enough of the daemon: hello, health, subscribe, idempotent writes (protocol 4.0).
class FakeDaemon {
  final List<FakeSocket> sockets = [];
  final Map<String, Map<String, dynamic>> receipts = {};
  final List<String> executed = [];
  bool reachable = true;

  /// When set, the next command is swallowed (connection dies before the answer).
  bool dropNextCommand = false;
  String environment = 'production';

  /// The protocol major the fake server answers hello with.
  int helloProtocol = 4;

  /// The major the fake server accepts in requests (a newer server refuses ours).
  int serverProtocol = 4;

  /// The params of the last hello.
  Map<String, dynamic>? hello;

  /// Board members (`UserView` json) and their passwords, as the server knows them.
  final List<Map<String, dynamic>> users = [];
  final Map<String, String> passwords = {};

  /// Every request as sent, to check what leaves the phone.
  final List<String> frames = [];

  void addUser(String id, String username, String password) {
    users.add({'id': id, 'username': username, 'created_at_ms': users.length});
    passwords[id] = password;
  }

  Map<String, dynamic>? _user(String? id) => users.where((u) => u['id'] == id).firstOrNull;
  Map<String, dynamic> _whoami(FakeSocket socket) => {'board_members_exist': users.isNotEmpty, 'user': ?_user(socket.user)};

  FakeSocket? get current => sockets.isEmpty ? null : sockets.last;

  Future<LinkSocket> connect(Uri uri, Map<String, String> headers) async {
    if (!reachable) throw Exception('unreachable');
    final socket = FakeSocket(this);
    sockets.add(socket);
    return socket;
  }

  void _handle(FakeSocket socket, Map<String, dynamic> request) {
    final id = request['request_id'] as int;
    void fail(String code, String message) => scheduleMicrotask(() => socket.push({
          'type': 'response',
          'request_id': id,
          'error': {'code': code, 'message': message},
        }));
    final params = request['params'] as Map<String, dynamic>?;
    void reply(String type, Object? value) => scheduleMicrotask(() => socket.push({
          'type': 'response',
          'request_id': id,
          'ok': {'type': type, 'value': value},
        }));
    if (request['protocol'] != serverProtocol) {
      scheduleMicrotask(() => socket.push({
            'type': 'response',
            'request_id': id,
            'error': {'code': 'incompatible_protocol', 'message': 'Update needed: this app speaks protocol ${request['protocol']}, the server $serverProtocol. Update the app.'},
          }));
      return;
    }
    switch (request['method']) {
      case 'session.hello':
        hello = request['params'] as Map<String, dynamic>?;
        socket.user = _user(hello?['user_id'] as String?)?['id'] as String?;
        reply('hello', {
          ..._whoami(socket),
          'host_id': 'host-1',
          'host_name': 'my-pc',
          'daemon_version': '0.7.0',
          'os': 'windows',
          'remote': true,
          'protocol': helloProtocol,
          'capabilities': ['read', 'chat', 'task_control', 'approve'],
          'environment': environment,
        });
      case 'server.get_health':
        reply('health', {'host_id': 'host-1', 'daemon_version': '0.7.0', 'pid': 1, 'started_at_ms': 0, 'last_seq': 10, 'active_jobs': 0});
      case 'event.subscribe':
        reply('subscribed', {'replayed_through_seq': 10});
      case 'user.list':
        reply('users', {'users': users});
      case 'user.whoami':
        reply('who_am_i', _whoami(socket));
      case 'user.use':
        if (++socket.tries > 5) return fail('too_many_attempts', 'Too many tries. Wait a minute and try again.');
        final userId = params!['user_id'] as String;
        if (passwords[userId] == null || passwords[userId] != params['password']) return fail('wrong_password', 'That board member or password is wrong.');
        socket.user = userId;
        reply('who_am_i', _whoami(socket));
      case 'user.create':
        final username = params!['username'] as String;
        if (users.any((u) => (u['username'] as String).toLowerCase() == username.toLowerCase())) return fail('username_taken', 'This username is taken.');
        if ((params['password'] as String).length < 12) return fail('weak_password', 'At least 12 characters.');
        final userId = 'u-${users.length + 1}';
        addUser(userId, username, params['password'] as String);
        socket.user ??= userId;
        if (dropNextCommand) {
          dropNextCommand = false;
          // Made on the PC, but the answer never arrives.
          scheduleMicrotask(() => socket.drop());
          return;
        }
        reply('receipt', {
          'idempotency_key': request['idempotency_key'],
          'committed_seq': 11,
          'replayed': false,
          'result': {
            'kind': 'command',
            'value': {'type': 'user_created', 'user_id': userId},
          },
        });
      case 'ticket.create':
        if (socket.user == null) return fail('board_member_required', 'A board member is needed for this.');
        reply('receipt', _receipt(request['idempotency_key'] as String, false));
      case 'employee.send_message' || 'task.cancel' || 'approval.decide':
        final key = request['idempotency_key'] as String;
        if (dropNextCommand) {
          dropNextCommand = false;
          // Applied on the PC, but the answer never arrives.
          receipts.putIfAbsent(key, () => _receipt(key, false));
          executed.add(key);
          scheduleMicrotask(() => socket.drop());
          return;
        }
        final replayed = receipts.containsKey(key);
        if (!replayed) {
          receipts[key] = _receipt(key, false);
          executed.add(key);
        }
        reply('receipt', {...receipts[key]!, 'replayed': replayed});
      default:
        reply('projects', {'as_of_seq': 10, 'projects': <Object>[]});
    }
  }

  Map<String, dynamic> _receipt(String key, bool replayed) => {
        'idempotency_key': key,
        'committed_seq': 11,
        'replayed': replayed,
        'result': {
          'kind': 'command',
          'value': {'type': 'employee_message_sent', 'conversation_id': 'conv-1', 'message_id': 'm-1', 'run_id': 'r-1'},
        },
      };
}

Future<MemoryHostStore> storeWithSecret() async {
  final store = MemoryHostStore();
  await store.save(testHost, 'secret');
  return store;
}

/// Wait until [condition] holds (or fail after a second).
Future<void> until(bool Function() condition) async {
  for (var i = 0; i < 200; i++) {
    if (condition()) return;
    await Future<void>.delayed(const Duration(milliseconds: 5));
  }
  throw StateError('condition not reached');
}
