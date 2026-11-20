import 'dart:async';
import 'dart:convert';
import 'dart:io' show Platform;
import 'dart:math';

import 'package:equatable/equatable.dart';
import 'package:uuid/uuid.dart';
import 'package:web_socket_channel/io.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import '../../data/protocol/protocol.dart';
import '../hosts/host.dart';
import '../hosts/host_store.dart';
import 'compacting.dart';
import 'gateway_api.dart';
import 'protocol.dart';

enum LinkStatus { connecting, online, offline, revoked, incompatible }

/// What the sign-in gate shows (protocol 5, MU3-02/06): nothing before the
/// first hello, then the first board member, the sign-in screen, a temporary
/// password to change, or the app.
enum BoardMemberStep { unknown, createFirst, signIn, changePassword, ready }

/// What the app shows about the PC connection (honest status,
/// which host, and how fresh the data is).
class LinkState extends Equatable {
  const LinkState({
    required this.status,
    this.message,
    this.lastOnlineMs,
    this.retryAtMs,
    this.capabilities = const [],
    this.daemonVersion,
    this.features = const [],
    this.boardMembersExist,
    this.user,
    this.mustChangePassword = false,
    this.claudeLogin,
    this.signedOut,
    this.hostName,
  });

  final LinkStatus status;
  final String? message;

  /// When the data was last known to be current.
  final int? lastOnlineMs;
  final int? retryAtMs;
  final List<String> capabilities;
  final String? daemonVersion;

  /// Additions to the protocol the server has (`HelloResult.features`); older servers send none.
  final List<String> features;

  /// The server has board members; null until the first hello.
  final bool? boardMembersExist;

  /// The board member of this phone's session.
  final UserView? user;

  /// [user] signed in with a temporary password and must change it first (B10).
  final bool mustChangePassword;

  /// [user]'s own Claude login (MU3-09); none without a session.
  final ClaudeLoginView? claudeLogin;

  /// The session ended without this phone asking (revoked, expired, a new
  /// password, disabled): "You were signed out on this device." until seen.
  final SignOutReason? signedOut;

  /// The server's name as it last said (hello, `server.get_name`); null
  /// before the first hello. The same as the connection's `Host.name`.
  final String? hostName;

  /// A board member may rename the server here (SN-01 AC5): the server has
  /// the feature, this phone is signed in and has the Owner right.
  bool get canRenameServer => has(serverNameFeature) && user != null && can('owner');

  BoardMemberStep get boardMemberStep => switch ((boardMembersExist, user)) {
        (_, UserView()) => mustChangePassword ? BoardMemberStep.changePassword : BoardMemberStep.ready,
        (null, _) => BoardMemberStep.unknown,
        (false, _) => BoardMemberStep.createFirst,
        (true, _) => BoardMemberStep.signIn,
      };

  bool get isOnline => status == LinkStatus.online;
  bool can(String capability) => capabilities.contains(capability);
  bool has(String feature) => features.contains(feature);

  LinkState copyWith({
    LinkStatus? status,
    String? message,
    int? lastOnlineMs,
    int? retryAtMs,
    List<String>? capabilities,
    String? daemonVersion,
    List<String>? features,
    String? hostName,
  }) =>
      LinkState(
        status: status ?? this.status,
        message: message,
        lastOnlineMs: lastOnlineMs ?? this.lastOnlineMs,
        retryAtMs: retryAtMs,
        capabilities: capabilities ?? this.capabilities,
        daemonVersion: daemonVersion ?? this.daemonVersion,
        features: features ?? this.features,
        boardMembersExist: boardMembersExist,
        user: user,
        mustChangePassword: mustChangePassword,
        claudeLogin: claudeLogin,
        signedOut: signedOut,
        hostName: hostName ?? this.hostName,
      );

  /// The same link with what the server said about the session's board member.
  LinkState withBoardMember({required bool exists, UserView? user, bool mustChangePassword = false, ClaudeLoginView? claudeLogin, SignOutReason? signedOut}) => LinkState(
        status: status,
        message: message,
        lastOnlineMs: lastOnlineMs,
        retryAtMs: retryAtMs,
        capabilities: capabilities,
        daemonVersion: daemonVersion,
        features: features,
        boardMembersExist: exists || user != null,
        user: user,
        mustChangePassword: user != null && mustChangePassword,
        claudeLogin: user == null ? null : claudeLogin,
        signedOut: user == null ? signedOut : null,
        hostName: hostName,
      );

  /// The same link with [login] as the own Claude login.
  LinkState withClaudeLogin(ClaudeLoginView? login) => user == null
      ? this
      : withBoardMember(exists: true, user: user, mustChangePassword: mustChangePassword, claudeLogin: login);

  @override
  List<Object?> get props => [
        status,
        message,
        lastOnlineMs,
        retryAtMs,
        capabilities,
        daemonVersion,
        features,
        boardMembersExist,
        user?.id,
        user?.username,
        mustChangePassword,
        claudeLogin?.status,
        claudeLogin?.email,
        claudeLogin?.plan,
        claudeLogin?.linkId,
        claudeLogin?.checkedAtMs,
        signedOut,
        hostName,
      ];
}

/// `HelloResult.features`: the server's name can be read and set (SN-01).
const serverNameFeature = 'server_name';

/// The write was not delivered and will not be retried automatically.
/// Resend with the same [idempotencyKey] to apply it at most once.
class NotSentException implements Exception {
  const NotSentException(this.idempotencyKey, {this.sent = false});

  final String idempotencyKey;

  /// It went out before the connection dropped: the PC may have applied it,
  /// only its answer never came.
  final bool sent;

  @override
  String toString() => sent ? 'The connection dropped before the PC answered.' : 'Not sent. The PC was not reachable.';
}

/// A minimal socket so tests can replace the network.
abstract class LinkSocket {
  Stream<dynamic> get stream;
  void send(String text);
  Future<void> close();
  int? get closeCode;
}

typedef SocketFactory = Future<LinkSocket> Function(Uri uri, Map<String, String> headers);

class _ChannelSocket implements LinkSocket {
  _ChannelSocket(this.channel);

  final WebSocketChannel channel;

  @override
  Stream<dynamic> get stream => channel.stream;

  @override
  void send(String text) => channel.sink.add(text);

  @override
  Future<void> close() async => channel.sink.close();

  @override
  int? get closeCode => channel.closeCode;
}

Future<LinkSocket> _connectSocket(Uri uri, Map<String, String> headers) async {
  final channel = IOWebSocketChannel.connect(uri, headers: headers, pingInterval: const Duration(seconds: 25), connectTimeout: const Duration(seconds: 12));
  await channel.ready;
  return _ChannelSocket(channel);
}

class _Pending {
  _Pending(this.request, this.idempotencyKey, this.expectedVersion, this.retry, this.timeout) : createdAt = DateTime.now();

  final Map<String, dynamic> request;
  final String idempotencyKey;
  final int? expectedVersion;
  final Duration? timeout;
  final bool retry;
  final DateTime createdAt;
  final Completer<Receipt> completer = Completer<Receipt>();
  bool inFlight = false;

  /// Went out at least once: the PC may have applied it.
  bool sent = false;
}

/// One live link to one host: session, WebSocket, hello, subscription,
/// reconnect with backoff and an outbox of unconfirmed writes.
class AureliusConnection {
  AureliusConnection({
    required Host host,
    required HostStore store,
    GatewayApi? api,
    SocketFactory? connect,
    this.clientName = 'aurelius-mobile',
    this.clientVersion = '0.7.0',
    String? deviceName,
  })  : _host = host, // ignore: prefer_initializing_formals
        _store = store, // ignore: prefer_initializing_formals
        deviceName = deviceName ?? (Platform.isIOS ? 'iPhone' : Platform.isAndroid ? 'Android phone' : 'Phone'),
        api = api ?? GatewayApi(),
        _connect = connect ?? _connectSocket;

  /// The host this connection talks to, under the name the server last said.
  Host get host => _host;
  Host _host;
  final GatewayApi api;
  final HostStore _store;
  final SocketFactory _connect;
  final String clientName;
  final String clientVersion;

  /// What the sessions list calls this phone (`LoginParams.device_name`).
  final String deviceName;

  /// Auto-retry window for unconfirmed commands (nothing is
  /// replayed silently much later).
  static const retryWindow = Duration(seconds: 60);
  static const _uuid = Uuid();

  final _states = StreamController<LinkState>.broadcast();
  final _events = StreamController<Event>.broadcast();
  final _appends = StreamController<StreamAppend>.broadcast();
  final _loads = StreamController<LoadSnapshot>.broadcast();
  final _powers = StreamController<PowerState>.broadcast();
  final _prompts = StreamController<ClaudeLoginPrompt>.broadcast();
  final _serverNames = StreamController<ServerNameView>.broadcast();
  final Map<int, Completer<ResponseBody>> _waiting = {};
  final List<_Pending> _outbox = [];

  /// Each streaming message's tool calls so far, merged from the changes the
  /// server sends (`activity_changes`), so [appends] always carry whole lists.
  final Map<String, List<ActivityView>> _calls = {};

  LinkState _state = const LinkState(status: LinkStatus.connecting);
  Session? _session;
  LinkSocket? _socket;
  StreamSubscription<dynamic>? _socketSub;
  Completer<void>? _closed;
  Completer<void> _wake = Completer<void>();
  Timer? _expiry;
  int _nextId = 1;
  bool _started = false;
  bool _disposed = false;

  LinkState get state => _state;
  Stream<LinkState> get states => _states.stream;
  Stream<Event> get events => _events.stream;
  Stream<StreamAppend> get appends => _appends.stream;

  /// `load_changed` snapshots.
  Stream<LoadSnapshot> get loads => _loads.stream;

  /// `power_changed` states.
  Stream<PowerState> get powers => _powers.stream;

  /// What `claude auth login` asks during this connection's Connect
  /// (`claude_login_prompt`). Never logged or kept: the link and the code
  /// live in the Connect sheet only.
  Stream<ClaudeLoginPrompt> get claudeLoginPrompts => _prompts.stream;

  /// Each `server.get_name` answer: the first read and every `server.renamed`.
  Stream<ServerNameView> get serverNames => _serverNames.stream;

  void _emit(LinkState next) {
    if (_disposed || next == _state) return;
    _state = next;
    _states.add(next);
  }

  void start() {
    if (_started) return;
    _started = true;
    _expiry = Timer.periodic(const Duration(seconds: 5), (_) => _expireOutbox());
    unawaited(_run());
  }

  /// App came to the foreground or the user tapped "Retry": try now.
  void wake() {
    if (!_wake.isCompleted) _wake.complete();
  }

  Future<void> dispose() async {
    _disposed = true;
    _expiry?.cancel();
    wake();
    await _socketSub?.cancel();
    await _socket?.close();
    _failWaiting();
    for (final p in _outbox) {
      if (!p.completer.isCompleted) p.completer.completeError(NotSentException(p.idempotencyKey));
    }
    _outbox.clear();
    await _states.close();
    await _events.close();
    await _appends.close();
    await _loads.close();
    await _powers.close();
    await _prompts.close();
    await _serverNames.close();
  }

  Future<void> _run() async {
    var attempt = 0;
    while (!_disposed) {
      _emit(_state.copyWith(status: LinkStatus.connecting));
      try {
        await _open();
        attempt = 0;
        await _closed!.future;
        if (_disposed) return;
        _emit(_state.copyWith(status: LinkStatus.offline, message: 'Connection lost. Reconnecting…'));
        if (_socket?.closeCode == 4401) {
          // Revoked, or remote access turned off on the PC: find out which, now.
          _session = null;
          continue;
        }
      } on RevokedException catch (e) {
        _emit(_state.copyWith(status: LinkStatus.revoked, message: e.toString()));
        return;
      } on _Incompatible catch (e) {
        _emit(_state.copyWith(status: LinkStatus.incompatible, message: e.message));
        return;
      } catch (e) {
        _session = e is SessionExpiredException ? null : _session;
        final message = e is HostUnreachableException ? e.message : _describe(e);
        _emit(_state.copyWith(status: LinkStatus.offline, message: message));
      }
      if (_disposed) return;
      final delay = _backoff(attempt++);
      _emit(_state.copyWith(status: LinkStatus.offline, message: _state.message, retryAtMs: DateTime.now().add(delay).millisecondsSinceEpoch));
      _wake = Completer<void>();
      await Future.any([Future<void>.delayed(delay), _wake.future]);
    }
  }

  static Duration _backoff(int attempt) {
    const steps = [1, 2, 4, 8, 15, 30];
    final base = steps[min(attempt, steps.length - 1)];
    return Duration(milliseconds: base * 1000 + Random().nextInt(400));
  }

  String _describe(Object e) {
    final text = e.toString();
    if (text.contains('was not upgraded') || text.contains('401')) return 'Session ended. Signing in again…';
    return 'The PC is not reachable. Is it on, with Aurelius and Tailscale running?';
  }

  Future<String> sessionToken() async {
    final current = _session;
    if (current != null && !current.expiresSoon) return current.token;
    final secret = await _store.secret(host.hostId);
    if (secret == null) throw const RevokedException();
    final session = await api.session(host, secret);
    _session = session;
    return session.token;
  }

  Future<void> _open() async {
    final token = await sessionToken();
    LinkSocket socket;
    try {
      socket = await _connect(host.websocketUri, {'authorization': 'Bearer $token'});
    } catch (_) {
      // Most likely an expired or revoked session: next round asks for a new one.
      _session = null;
      rethrow;
    }
    _socket = socket;
    _closed = Completer<void>();
    _socketSub = socket.stream.listen(
      _onFrame,
      onDone: _onClosed,
      onError: (_) => _onClosed(),
      cancelOnError: true,
    );

    final HelloResult hello;
    try {
      _calls.clear(); // a new session starts with whole lists
      compactingConversations.value = const {};
      final token = await _store.sessionToken(host.hostId);
      HelloResult answer;
      try {
        answer = await _hello(token);
      } on ProtocolException catch (e) {
        if (e.code != ErrorCode.unauthenticated || token == null) rethrow;
        // The server refused the kept session: forget it, hello without one.
        await _store.setSessionToken(host.hostId, null);
        answer = await _hello(null);
      }
      hello = answer;
      // A kept session the server no longer has (expired, revoked meanwhile,
      // a restore): forget it; the sign-in screen comes (MU3-03 AC3).
      if (token != null && hello.user == null) {
        await _store.setSessionToken(host.hostId, null);
        if (_state.user != null && hello.boardMembersExist) _signedOutNotice = SignOutReason.expired;
      }
    } on ProtocolException catch (e) {
      if (e.code != ErrorCode.incompatibleProtocol) rethrow;
      throw const _Incompatible(differentVersions);
    }
    if (hello.protocol != protocolMajor) {
      await socket.close();
      throw const _Incompatible(differentVersions);
    }
    if (hello.environment.isNotEmpty && hello.environment != host.environment) {
      await socket.close();
      throw _Incompatible('This phone is paired with the ${hello.environment} Aurelius on ${host.name}, which changed. Pair again.');
    }
    // The gate (protocol 5) answers health and events only with a session:
    // without one they follow the sign-in (or the new password).
    _subscribed = false;
    if (hello.user != null && !(hello.mustChangePassword ?? false)) await _subscribe();
    // Every hello brings the server's name; a rename while away lands here (SN-02 AC2).
    await _rename(hello.hostName);
    final now = DateTime.now().millisecondsSinceEpoch;
    _emit(LinkState(
      status: LinkStatus.online,
      lastOnlineMs: now,
      capabilities: [for (final c in hello.capabilities) c.wire],
      daemonVersion: hello.daemonVersion,
      features: hello.features,
      boardMembersExist: hello.boardMembersExist || hello.user != null,
      user: hello.user,
      mustChangePassword: hello.user != null && (hello.mustChangePassword ?? false),
      claudeLogin: hello.user == null ? null : hello.claudeLogin,
      signedOut: hello.user == null ? _signedOutNotice : null,
      hostName: _host.name,
    ));
    if (hello.user != null) _signedOutNotice = null;
    _flushOutbox();
  }

  /// This socket gets the events (`event.subscribe`).
  bool _subscribed = false;

  /// Subscribe this socket to the events from the server's last seq, once.
  Future<void> _subscribe() async {
    if (_subscribed) return;
    final health = (await call(serverGetHealth()) as ResponseBodyHealth).value;
    await call(eventSubscribe(afterSeq: health.lastSeq));
    _subscribed = true;
  }

  Future<HelloResult> _hello(String? token) async {
    final body = await call(sessionHello(HelloParams(clientName: clientName, clientVersion: clientVersion, accepts: const ['activity_changes', 'server_name', 'ticket_channels', 'chat_threads', 'chat_reactions', 'ticket_follow', 'message_edits'], sessionToken: token)));
    return (body as ResponseBodyHello).value;
  }

  /// "You were signed out on this device." waits here across the reconnect
  /// that follows `signed_out`, until the sign-in screen showed it.
  SignOutReason? _signedOutNotice;

  /// The session ended on the server's side: forget its token at once (S8) and
  /// go to the sign-in screen with [reason] as the notice.
  Future<void> _endSession(SignOutReason? reason) async {
    // The server stops this connection's events with its board member (QA3
    // M2): the next sign-in on it subscribes again.
    _subscribed = false;
    await _store.setSessionToken(host.hostId, null);
    _signedOutNotice = reason;
    if (_state.user != null || reason != null) _emit(_state.withBoardMember(exists: true, signedOut: reason));
  }

  void _onFrame(dynamic frame) {
    if (frame is! String) return;
    final Object? decoded;
    try {
      decoded = jsonDecode(frame);
    } on FormatException {
      return;
    }
    if (decoded is! Map<String, dynamic>) return;
    final ServerMessage message;
    try {
      message = ServerMessage.fromJson(decoded);
    } on FormatException {
      return; // a message kind this app does not know
    } on TypeError {
      return;
    }
    switch (message) {
      case ServerMessageResponse():
        final error = message.error;
        final ok = message.ok;
        final waiting = _waiting.remove(message.requestId);
        if (error != null) {
          switch (error.code) {
            // The server has no session for this connection: back to the sign-in screen.
            case ErrorCode.unauthenticated when _state.user != null:
              unawaited(_endSession(SignOutReason.expired));
            case ErrorCode.boardMemberRequired when _state.user != null:
              unawaited(_endSession(null));
            case ErrorCode.passwordChangeRequired when _state.user != null && !_state.mustChangePassword:
              _emit(_state.withBoardMember(exists: true, user: _state.user, mustChangePassword: true, claudeLogin: _state.claudeLogin));
            default:
              break;
          }
          waiting?.completeError(ProtocolException(error.code, error.message, retryAfterMs: error.retryAfterMs, matchedItems: error.matchedItems, tooLarge: error.tooLarge));
        } else if (ok != null) {
          waiting?.complete(ok);
        }
      case ServerMessageEvent():
        // The server forgets what it sent on these, so the next update is whole again.
        if (message.value.kind.wire.startsWith('message.') || message.value.kind.wire.startsWith('run.')) _calls.clear();
        if (message.value.kind == EventKind.claudeLoginUpdated && _state.user != null) unawaited(refreshClaudeLogin().catchError((Object _) => _state.claudeLogin));
        // The next hello brings the name anyway when this read fails.
        if (message.value.kind == EventKind.serverRenamed && _state.has(serverNameFeature)) unawaited(refreshServerName().then((_) {}, onError: (Object _) {}));
        _events.add(message.value);
        if (_state.isOnline) _emit(_state.copyWith(status: LinkStatus.online, lastOnlineMs: DateTime.now().millisecondsSinceEpoch, message: _state.message));
      case ServerMessageStreamAppend():
        if (message.value.progress != null) noteCompacting(message.value.conversationId, message.value.progress!.compacting);
        _appends.add(_merged(message.value));
      case ServerMessageLoadChanged():
        _loads.add(message.value);
      case ServerMessagePowerChanged():
        _powers.add(message.value);
      case ServerMessageClaudeLoginPrompt():
        _prompts.add(message.value);
      case ServerMessageSignedOut():
        // A sign-out this phone asked for needs no message.
        unawaited(_endSession(_signingOut && message.reason == SignOutReason.loggedOut ? null : message.reason));
      case ServerMessageStream():
        break; // local IPC only
    }
  }

  /// [append] with its message's whole tool list: changes (`activity_changes`)
  /// replace known calls in place and add new ones at the end.
  StreamAppend _merged(StreamAppend append) {
    final activity = append.activity;
    if (activity == null) return append; // no changes
    if (append.activityChanges != true) {
      _calls[append.messageId] = activity;
      return append;
    }
    final calls = [...?_calls[append.messageId]];
    for (final a in activity) {
      final at = calls.indexWhere((c) => c.callId == a.callId);
      if (at >= 0) {
        calls[at] = a;
      } else {
        calls.add(a);
      }
    }
    _calls[append.messageId] = calls;
    return StreamAppend(
      activity: calls,
      conversationId: append.conversationId,
      messageId: append.messageId,
      mode: append.mode,
      offset: append.offset,
      progress: append.progress,
      runId: append.runId,
      text: append.text,
      threadParent: append.threadParent,
    );
  }

  void _onClosed() {
    _failWaiting();
    for (final p in _outbox) {
      p.inFlight = false;
    }
    if (_closed != null && !_closed!.isCompleted) _closed!.complete();
  }

  void _failWaiting() {
    final waiting = Map.of(_waiting);
    _waiting.clear();
    for (final c in waiting.values) {
      if (!c.isCompleted) c.completeError(const DisconnectedException());
    }
  }

  /// Send a request built by the generated method functions (`{method, params}`);
  /// returns the answer. Throws [ProtocolException] or [DisconnectedException].
  /// [timeout]: for the few slow ones (a download).
  Future<ResponseBody> call(Map<String, dynamic> request, [Duration timeout = const Duration(seconds: 30)]) async {
    final socket = _socket;
    if (socket == null || _closed == null || _closed!.isCompleted) throw const DisconnectedException();
    final id = _nextId++;
    final completer = Completer<ResponseBody>();
    _waiting[id] = completer;
    socket.send(jsonEncode({'protocol': protocolMajor, 'request_id': id, ...request}));
    return completer.future.timeout(timeout, onTimeout: () {
      _waiting.remove(id);
      throw const DisconnectedException('The PC did not answer in time.');
    });
  }

  /// Write through the generated method functions. Unconfirmed writes are resent
  /// with the same idempotency key after a reconnect within [retryWindow] when
  /// [retry] is true; otherwise they fail with [NotSentException]. Approvals pass `retry: false`.
  Future<Receipt> write(
    Map<String, dynamic> request, {
    String? idempotencyKey,
    int? expectedVersion,
    bool retry = true,
    Duration? timeout,
  }) {
    final pending = _Pending(request, idempotencyKey ?? _uuid.v7(), expectedVersion, retry, timeout);
    _outbox.add(pending);
    if (_state.isOnline) {
      _send(pending);
    } else if (!retry) {
      _outbox.remove(pending);
      pending.completer.completeError(NotSentException(pending.idempotencyKey));
    }
    return pending.completer.future;
  }

  Future<void> _send(_Pending pending) async {
    if (pending.inFlight || pending.completer.isCompleted) return;
    pending.inFlight = true;
    pending.sent = true;
    try {
      final body = await call({
        ...pending.request,
        'idempotency_key': pending.idempotencyKey,
        'expected_version': ?pending.expectedVersion,
      }, pending.timeout ?? const Duration(seconds: 30));
      _outbox.remove(pending);
      pending.completer.complete((body as ResponseBodyReceipt).value);
    } on DisconnectedException {
      pending.inFlight = false;
      if (!pending.retry) {
        _outbox.remove(pending);
        pending.completer.completeError(NotSentException(pending.idempotencyKey, sent: true));
      }
    } catch (e) {
      _outbox.remove(pending);
      pending.completer.completeError(e);
    }
  }

  /// Sign in (`user.login`, MU3-02): the session's token goes to the
  /// Keychain / Keystore for the next hellos (S8). The password is only sent,
  /// never kept. Throws [ProtocolException] (`wrong_password`,
  /// `too_many_attempts` with its wait).
  Future<LoginResult> signIn(String username, String password) async {
    final body = await call(userLogin(LoginParams(deviceName: deviceName, password: password, username: username.trim())));
    final login = (body as ResponseBodyLogin).value;
    // A server still on the freeze gives no token yet: nothing to keep.
    await _store.setSessionToken(host.hostId, login.sessionToken.isEmpty ? null : login.sessionToken);
    _signedOutNotice = null;
    if (!login.mustChangePassword) await _subscribe();
    _emit(_state.withBoardMember(exists: true, user: login.user, mustChangePassword: login.mustChangePassword));
    unawaited(refreshClaudeLogin().catchError((Object _) => null));
    return login;
  }

  bool _signingOut = false;

  /// Sign out on this phone (`user.logout`, MU3-04 AC3): the token is
  /// forgotten even when the server cannot be told.
  Future<void> signOut() async {
    _signingOut = true;
    try {
      await call(userLogout());
    } on DisconnectedException {
      // Offline: forget it here; the server ends it after 30 days unused.
    } on ProtocolException catch (e) {
      if (e.code != ErrorCode.unauthenticated) rethrow;
    } finally {
      _signingOut = false;
    }
    await _endSession(null);
  }

  /// Change a password (`user.set_password`). Own ([userId] null): with
  /// [oldPassword]; the other sessions end. Another board member's (B10): a
  /// temporary one. Never resent after a reconnect, so no password waits in
  /// the outbox.
  Future<void> setPassword({required String newPassword, String? oldPassword, String? userId}) async {
    await write(userSetPassword(SetPasswordParams(newPassword: newPassword, oldPassword: oldPassword, userId: userId)), retry: false);
    final me = _state.user;
    if (me != null && (userId == null || userId == me.id)) {
      // The temporary password is gone: now the gate lets the events through.
      if (_state.mustChangePassword) await _subscribe();
      _emit(_state.withBoardMember(exists: true, user: me, claudeLogin: _state.claudeLogin));
    }
  }

  /// A new board member (`user.create`). The first one (no session yet)
  /// signs in right after, for the session's token. Never resent after a
  /// reconnect, so the password does not wait in the outbox.
  Future<String> createBoardMember(String username, String password) async {
    final first = _state.user == null;
    final receipt = await write(userCreate(UserCreateParams(username: username, password: password)), retry: false);
    final created = ((receipt.result as WriteResultCommand).value as CommandResultUserCreated).userId;
    if (first) await signIn(username, password);
    return created;
  }

  /// The own Claude login, asked again (`claude_login.status`).
  Future<ClaudeLoginView?> refreshClaudeLogin() async {
    final body = await call(claudeLoginStatus());
    final login = (body as ResponseBodyClaudeLogin).value;
    _emit(_state.withClaudeLogin(login));
    return login;
  }

  /// The server's name, asked again (`server.get_name`; only with the
  /// feature `server_name`). It becomes [host]'s name, here and in the host
  /// store, and goes to [serverNames].
  Future<ServerNameView> refreshServerName() async {
    final view = (await call(serverGetName()) as ResponseBodyServerName).value;
    await applyServerName(view.name);
    if (!_serverNames.isClosed) _serverNames.add(view);
    return view;
  }

  /// [name] as the server's name (a `server.set_name` receipt, a read).
  Future<void> applyServerName(String name) async {
    await _rename(name);
    if (_state.hostName != _host.name) _emit(_state.copyWith(status: _state.status, message: _state.message, retryAtMs: _state.retryAtMs, hostName: _host.name));
  }

  /// [name] becomes [host]'s name and the stored one, when it differs.
  Future<void> _rename(String name) async {
    if (name.trim().isEmpty || name == _host.name || _disposed) return;
    _host = _host.withName(name);
    // The name is only shown: a store that cannot write it must not fail the hello.
    try {
      await _store.rename(_host.hostId, name);
    } catch (_) {}
  }

  /// [login] as the own Claude login (an answer that carries it).
  void applyClaudeLogin(ClaudeLoginView login) => _emit(_state.withClaudeLogin(login));

  /// The sign-in screen showed "You were signed out on this device.".
  void dismissSignedOut() {
    _signedOutNotice = null;
    if (_state.signedOut != null) _emit(_state.withBoardMember(exists: _state.boardMembersExist ?? true));
  }

  /// Take back a command that never left this phone: it fails with
  /// [NotSentException]. False when it went out already (the PC may have
  /// applied it; its receipt still arrives) or is not in the outbox.
  bool withdraw(String idempotencyKey) {
    final pending = _outbox.where((p) => p.idempotencyKey == idempotencyKey).firstOrNull;
    if (pending == null || pending.sent) return false;
    _outbox.remove(pending);
    if (!pending.completer.isCompleted) pending.completer.completeError(NotSentException(pending.idempotencyKey));
    return true;
  }

  void _flushOutbox() {
    for (final pending in List.of(_outbox)) {
      _send(pending);
    }
  }

  void _expireOutbox() {
    final now = DateTime.now();
    for (final pending in List.of(_outbox)) {
      if (!pending.inFlight && now.difference(pending.createdAt) > retryWindow) {
        _outbox.remove(pending);
        if (!pending.completer.isCompleted) pending.completer.completeError(NotSentException(pending.idempotencyKey));
      }
    }
  }
}

/// Shown when the PC speaks another protocol major (3 since the company org).
const differentVersions = 'Update Aurelius on the phone and the PC.';

class _Incompatible implements Exception {
  const _Incompatible(this.message);

  final String message;
}
