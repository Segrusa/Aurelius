import 'dart:async';
import 'dart:convert';

import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/hosts/host_store.dart';
import 'package:aurelius_mobile/core/net/connection.dart';
import 'package:aurelius_mobile/core/net/gateway_api.dart';
import 'package:aurelius_mobile/data/views.dart';

import 'tickets.dart' show boardOf, ticketOf;

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

  /// This connection's session's board member (the daemon's `Peer.user`).
  String? user;

  /// This connection's session token, when it has one.
  String? token;
  int tries = 0;

  /// This connection gets the events (`event.subscribe`); the server stops
  /// them when its board member goes (sign-out), as `api::run` does.
  bool subscribed = false;

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

/// Just enough of the daemon: hello, health, subscribe, idempotent writes,
/// sign-in and sessions (protocol 5).
class FakeDaemon {
  final List<FakeSocket> sockets = [];
  final Map<String, Map<String, dynamic>> receipts = {};
  final List<String> executed = [];
  bool reachable = true;

  /// When set, the next command is swallowed (connection dies before the answer).
  bool dropNextCommand = false;
  String environment = 'production';

  /// The protocol major the fake server answers hello with.
  int helloProtocol = 5;

  /// The major the fake server accepts in requests (a newer server refuses ours).
  int serverProtocol = 5;

  /// The params of the last hello.
  Map<String, dynamic>? hello;

  /// Board members (`UserView` json) and their passwords, as the server knows them.
  final List<Map<String, dynamic>> users = [];
  final Map<String, String> passwords = {};

  /// Every request as sent, to check what leaves the phone.
  final List<String> frames = [];

  /// Sessions by token: `{id, user_id, device_name}`.
  final Map<String, Map<String, dynamic>> sessions = {};

  /// How many times a connection subscribed to the events.
  int subscribes = 0;

  /// Board members whose next sign-in must change the password (B10).
  final Set<String> mustChange = {};

  /// Wrong sign-ins per username; from the 6th a wait (B9).
  final Map<String, int> failures = {};

  /// Methods this server answers as a freeze placeholder ("cannot do this yet").
  final Set<String> notYet = {};

  /// Methods this server refuses with an error code (e.g. `password_change_required`).
  final Map<String, String> refuse = {};

  /// Methods this server refuses with a whole error (`AppError`, e.g. `looks_like_secret` with `matched_items`).
  final Map<String, Map<String, dynamic>> errors = {};

  /// The freeze's user.login: a session on the connection, but no token yet.
  bool emptyTokens = false;

  /// The Claude login status the server reports (`missing`, `linked`, `expired`).
  String claudeStatus = 'missing';

  /// The Claude link codes the server got, to check they went out once.
  final List<String> claudeCodes = [];

  /// As the real server: `claude_login.input` answers the status before
  /// Claude finished; the link follows a moment later with `claude_login.updated`.
  bool linksAfterInput = false;

  /// The tickets on the Board (`board.get`); `ticket.create` adds one and
  /// tells every subscribed connection (`ticket.created`).
  final List<TicketView> tickets = [];

  /// Ticket ids `ticket.use_my_login` was asked for.
  final List<String> usedMyLogin = [];

  /// What hello says: the rights of this connection and the server's features.
  List<String> capabilities = const ['read', 'chat', 'task_control', 'approve'];
  List<String> features = const [];

  /// The server's name (feature `server_name`): the computer's, and the one a
  /// board member chose; hello's `host_name` is the chosen one, else the computer's.
  String computerName = 'my-pc';
  String? chosenName;
  int nameVersion = 0;
  String get hostName => chosenName ?? computerName;

  /// How many times `server.get_name` was asked.
  int nameReads = 0;

  void addUser(String id, String username, String password) {
    users.add({'id': id, 'username': username, 'created_at_ms': users.length});
    passwords[id] = password;
  }

  Map<String, dynamic>? _user(String? id) => users.where((u) => u['id'] == id).firstOrNull;
  Map<String, dynamic> _whoami(FakeSocket socket) => {
        'board_members_exist': users.isNotEmpty,
        'user': ?_user(socket.user),
        if (socket.user != null) 'must_change_password': mustChange.contains(socket.user),
      };
  Map<String, dynamic> claudeView(String userId) => {
        'user_id': userId,
        'status': claudeStatus,
        'background': false,
        if (claudeStatus == 'linked') 'email': 'test@example.com',
        if (claudeStatus == 'linked') 'plan': 'max',
      };

  /// A session for [userId] that the phone can bring in its hello.
  String sessionFor(String userId, {String device = 'iPhone'}) {
    final token = 'aus_${sessions.length + 1}';
    sessions[token] = {'id': 's-${sessions.length + 1}', 'user_id': userId, 'device_name': device, 'created_at_ms': 0, 'last_seen_ms': 0, 'expires_at_ms': 0};
    return token;
  }

  /// The server ends [socket]'s session (`signed_out`) and closes the connection.
  Future<void> signOut(FakeSocket socket, String reason) async {
    sessions.remove(socket.token);
    socket.user = null;
    socket.push({'type': 'signed_out', 'reason': reason});
    await socket.drop();
  }

  /// What `claude auth login` asks, to [socket] only.
  void prompt(FakeSocket socket, {required String linkId, String? url, bool asksCode = false, String? error}) =>
      socket.push({'type': 'claude_login_prompt', 'link_id': linkId, 'url': ?url, 'asks_code': asksCode, 'error': ?error});

  /// An event for [socket]; only a subscribed connection gets it.
  void event(FakeSocket socket, String kind, {int seq = 11}) {
    if (socket.subscribed) _event(socket, kind, seq);
  }

  void _event(FakeSocket socket, String kind, int seq) => socket.push({
        'type': 'event',
        'seq': seq,
        'id': 'ev-$seq',
        'kind': kind,
        'aggregate_type': 'user',
        'aggregate_id': socket.user ?? 'u-1',
        'payload': <String, dynamic>{},
        'at_ms': 0,
      });

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
    final method = request['method'] as String;
    if (notYet.contains(method)) return fail('validation', 'This server cannot do this yet.');
    if (refuse[method] case final code?) return fail(code, '');
    if (errors[method] case final error?) return scheduleMicrotask(() => socket.push({'type': 'response', 'request_id': id, 'error': error}));
    void receipt(Map<String, dynamic> result) => reply('receipt', {
          'idempotency_key': request['idempotency_key'],
          'committed_seq': 11,
          'replayed': false,
          'result': result,
        });
    switch (method) {
      case 'session.hello':
        hello = request['params'] as Map<String, dynamic>?;
        final token = hello?['session_token'] as String?;
        final session = sessions[token];
        socket.token = session == null ? null : token;
        socket.user = session?['user_id'] as String?;
        reply('hello', {
          ..._whoami(socket),
          'host_id': 'host-1',
          'host_name': hostName,
          'daemon_version': '0.7.0',
          'os': 'windows',
          'remote': true,
          'protocol': helloProtocol,
          'capabilities': capabilities,
          'features': features,
          'environment': environment,
          if (socket.user != null) 'claude_login': claudeView(socket.user!),
        });
      // The gate: health and events only with a session whose password is not temporary.
      case 'server.get_health' || 'event.subscribe' when socket.user == null:
        return fail('unauthenticated', 'Sign in first.');
      case 'server.get_health' || 'event.subscribe' when mustChange.contains(socket.user):
        return fail('password_change_required', 'Change your temporary password first.');
      case 'server.get_health':
        reply('health', {'host_id': 'host-1', 'daemon_version': '0.7.0', 'pid': 1, 'started_at_ms': 0, 'last_seq': 10, 'active_jobs': 0});
      case 'event.subscribe':
        subscribes++;
        socket.subscribed = true;
        reply('subscribed', {'replayed_through_seq': 10});
      case 'server.get_name':
        nameReads++;
        reply('server_name', {'name': hostName, 'chosen': ?chosenName, 'computer_name': computerName, 'version': nameVersion});
      case 'server.set_name':
        if (socket.user == null) return fail('board_member_required', 'A board member is needed for this.');
        if (!capabilities.contains('owner')) return fail('forbidden', 'This needs the Owner right.');
        final name = (params!['name'] as String).trim();
        if (name.length > 40) return fail('validation', 'The server name has at most 40 characters.');
        if (name.contains(RegExp(r'[\x00-\x1f\x7f]'))) return fail('validation', 'The server name cannot hold line breaks or control characters.');
        final next = name.isEmpty ? null : name;
        if (next != chosenName) {
          chosenName = next;
          nameVersion++;
          for (final s in sockets) {
            event(s, 'server.renamed', seq: 30 + nameVersion);
          }
        }
        receipt({'kind': 'command', 'value': {'type': 'server_name_set', 'name': hostName, 'version': nameVersion}});
      case 'user.list':
        reply('users', {'users': users});
      case 'user.whoami':
        reply('who_am_i', _whoami(socket));
      case 'user.login':
        final username = (params!['username'] as String).toLowerCase();
        if ((failures[username] ?? 0) >= 5) {
          return scheduleMicrotask(() => socket.push({
                'type': 'response',
                'request_id': id,
                'error': {'code': 'too_many_attempts', 'message': 'Too many attempts.', 'retry_after_ms': 30000},
              }));
        }
        final user = users.where((u) => (u['username'] as String).toLowerCase() == username).firstOrNull;
        if (user == null || passwords[user['id']] != params['password']) {
          failures[username] = (failures[username] ?? 0) + 1;
          return fail('wrong_password', 'Username or password is wrong.');
        }
        failures.remove(username);
        final userId = user['id'] as String;
        final token = sessionFor(userId, device: params['device_name'] as String);
        socket.user = userId;
        socket.token = token;
        reply('login', {'user': user, 'session_token': emptyTokens ? '' : token, 'expires_at_ms': 0, 'must_change_password': mustChange.contains(userId)});
      case 'user.logout':
        sessions.remove(socket.token);
        socket.token = null;
        socket.user = null;
        socket.subscribed = false;
        reply('who_am_i', _whoami(socket));
      case 'user.set_password':
        if (socket.user == null) return fail('unauthenticated', 'Sign in first.');
        final target = (params!['user_id'] as String?) ?? socket.user!;
        if (target == socket.user) {
          if (passwords[target] != params['old_password']) return fail('wrong_password', 'The password is wrong.');
          mustChange.remove(target);
          sessions.removeWhere((t, s) => s['user_id'] == target && t != socket.token);
        } else {
          mustChange.add(target);
          sessions.removeWhere((t, s) => s['user_id'] == target);
        }
        if ((params['new_password'] as String).length < 12) return fail('weak_password', 'At least 12 characters.');
        passwords[target] = params['new_password'] as String;
        receipt({'kind': 'host', 'value': {'type': 'done'}});
      case 'user.disable':
        final target = params!['user_id'] as String;
        if (users.where((u) => u['disabled_at_ms'] == null).length <= 1) return fail('last_board_member', 'The last active board member cannot be disabled.');
        final user = _user(target)!..['disabled_at_ms'] = 5;
        sessions.removeWhere((t, s) => s['user_id'] == target);
        receipt({'kind': 'host', 'value': {'type': 'user', ...user}});
      case 'session.list':
        if (socket.user == null) return fail('unauthenticated', 'Sign in first.');
        final of = (params?['user_id'] as String?) ?? socket.user!;
        reply('sessions', {
          'user_id': of,
          'sessions': [
            for (final e in sessions.entries)
              if (e.value['user_id'] == of) {...e.value, 'current': e.key == socket.token},
          ],
        });
      case 'session.revoke':
        final sid = params!['session_id'] as String;
        sessions.removeWhere((t, s) => s['id'] == sid);
        receipt({'kind': 'host', 'value': {'type': 'done'}});
      case 'claude_login.status':
        if (socket.user == null) return fail('unauthenticated', 'Sign in first.');
        reply('claude_login', claudeView(socket.user!));
      case 'claude_login.link':
        receipt({'kind': 'host', 'value': {'type': 'claude_login_link', 'link_id': 'link-1'}});
      case 'claude_login.input':
        claudeCodes.add(params!['text'] as String);
        reply('claude_login', {...claudeView(socket.user!), if (linksAfterInput) 'link_id': params['link_id']});
        if (linksAfterInput) {
          Timer(const Duration(milliseconds: 20), () {
            claudeStatus = 'linked';
            event(socket, 'claude_login.updated', seq: 12);
          });
        }
      case 'claude_login.cancel':
        receipt({'kind': 'host', 'value': {'type': 'claude_login', ...claudeView(socket.user!)}});
      case 'claude_login.unlink':
        claudeStatus = 'missing';
        receipt({'kind': 'host', 'value': {'type': 'claude_login', ...claudeView(socket.user!)}});
      case 'ticket.use_my_login':
        usedMyLogin.add(params!['ticket_id'] as String);
        reply('receipt', _receipt(request['idempotency_key'] as String, false));
      case 'user.create':
        final username = params!['username'] as String;
        if (users.any((u) => (u['username'] as String).toLowerCase() == username.toLowerCase())) return fail('username_taken', 'This username is taken.');
        if ((params['password'] as String).length < 12) return fail('weak_password', 'At least 12 characters.');
        final userId = 'u-${users.length + 1}';
        addUser(userId, username, params['password'] as String);
        // The onboarding's connection gets a session; its token comes with user.login.
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
        final creator = UserView.fromJson(_user(socket.user)!);
        tickets.add(ticketOf(id: 'k${tickets.length + 1}', number: tickets.length + 1, title: params!['title'] as String, creator: creator));
        reply('receipt', _receipt(request['idempotency_key'] as String, false));
        for (final s in sockets) {
          event(s, 'ticket.created', seq: 20 + tickets.length);
        }
      case 'board.get':
        if (socket.user == null) return fail('unauthenticated', 'Sign in first.');
        reply('board', {...boardOf(tickets).toJson(), 'as_of_seq': 20 + tickets.length});
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
