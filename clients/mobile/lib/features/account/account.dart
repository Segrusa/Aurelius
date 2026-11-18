import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart';

import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart' show confirm;
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart' show toast;
import '../../data/aurelius_repository.dart';
import '../../data/protocol/protocol.dart';
import '../connection/connection_bloc.dart';
import '../onboarding/onboarding.dart' show BoardMemberRules, OtherServer;

// Protocol 5 on the phone (ui-ontwerp Fase 3 §1–§2): the sign-in screen, the
// change of a (temporary) password, Profile › Sessions. Passwords
// live in these pages' fields only: never logged, kept or shown in a toast.

/// The one answer for a wrong username or password (MU3-02 AC3).
const wrongSignIn = 'Username or password is wrong.';

/// "0:30", "14:59": a wait in minutes and seconds.
String waitClock(Duration wait) {
  final s = wait.inSeconds < 0 ? 0 : (wait.inMilliseconds / 1000).ceil();
  return '${s ~/ 60}:${(s % 60).toString().padLeft(2, '0')}';
}

/// "You were signed out on this device." and why (ui-ontwerp §1).
String signedOutReason(SignOutReason reason) => switch (reason) {
      SignOutReason.revoked => 'Another device ended this session.',
      SignOutReason.expired => 'The session expired.',
      SignOutReason.passwordChanged => 'The password was changed.',
      SignOutReason.disabled => 'This board member was disabled.',
      SignOutReason.loggedOut => 'You signed out.',
    };

/// The sign-in screen (MU3-02, ui-ontwerp §1 Mobiel): a full page without the
/// tab bar, one group, autofill hints, one generic error, and a countdown
/// after too many tries (B9).
class SignInPage extends StatefulWidget {
  const SignInPage({super.key});

  @override
  State<SignInPage> createState() => _SignInPageState();
}

class _SignInPageState extends State<SignInPage> {
  final _username = TextEditingController();
  final _password = TextEditingController();
  final _passwordFocus = FocusNode();
  String? _error;
  bool _busy = false;
  bool _shown = false;
  Timer? _tick;
  bool _noticeOpen = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _notice(context.read<ConnectionBloc>().state));
  }

  @override
  void dispose() {
    _tick?.cancel();
    _username.dispose();
    _password.dispose();
    _passwordFocus.dispose();
    super.dispose();
  }

  /// The session ended without this phone asking: say so once, modally.
  Future<void> _notice(LinkState link) async {
    final reason = link.signedOut;
    if (reason == null || _noticeOpen || !mounted) return;
    _noticeOpen = true;
    final repository = context.read<AureliusRepository>();
    await showDialog<void>(
      context: context,
      builder: (dialog) => AlertDialog(
        key: const ValueKey('signed-out-notice'),
        title: const Text('You were signed out on this device.'),
        content: Text('${signedOutReason(reason)} Sign in again to continue.'),
        actions: [TextButton(onPressed: () => Navigator.pop(dialog), child: const Text('OK'))],
      ),
    );
    _noticeOpen = false;
    repository.dismissSignedOut();
  }

  /// What is left of the server's wait (`retry_after_ms`), counted down here.
  Duration? get _wait => _left;
  Duration? _left;

  void _waitFor(int ms) {
    _left = Duration(milliseconds: ms);
    _tick?.cancel();
    _tick = Timer.periodic(const Duration(seconds: 1), (t) {
      if (!mounted) return t.cancel();
      setState(() {
        final left = _left! - const Duration(seconds: 1);
        if (left <= Duration.zero) {
          t.cancel();
          _left = null;
          _error = null;
        } else {
          _left = left;
        }
      });
    });
  }

  Future<void> _submit() async {
    if (_busy || _wait != null) return;
    if (_username.text.trim().isEmpty || _password.text.isEmpty) {
      setState(() => _error = 'Enter your username and password.');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    final messenger = ScaffoldMessenger.of(context);
    try {
      final login = await context.read<AureliusRepository>().signIn(_username.text, _password.text);
      _password.clear();
      // The gate replaces this page with the app (or the password change).
      if (!login.mustChangePassword) {
        messenger
          ..hideCurrentSnackBar()
          ..showSnackBar(SnackBar(content: Text('Welcome, ${login.user.username}.')));
      }
    } on ProtocolException catch (e) {
      if (!mounted) return;
      setState(() {
        switch (e.code) {
          case ErrorCode.wrongPassword || ErrorCode.notFound || ErrorCode.unauthenticated:
            _error = wrongSignIn;
          case ErrorCode.tooManyAttempts:
            _waitFor(e.retryAfterMs ?? 60000);
            _error = null;
          default:
            _error = e.shown;
        }
      });
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final host = context.read<AureliusRepository>().host;
    final wait = _wait;
    final error = wait != null ? 'Too many attempts. Try again in ${waitClock(wait)}.' : _error;
    return BlocListener<ConnectionBloc, LinkState>(
      listenWhen: (a, b) => a.signedOut != b.signedOut && b.signedOut != null,
      listener: (context, link) => _notice(link),
      child: LargeTitleScaffold(
        title: 'Sign in',
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 8, 20, 16),
            child: Text('to ${host.name} (tailnet)', style: TextStyle(fontSize: 15, color: Tokens.label2)),
          ),
          AutofillGroup(
            child: InsetGroup(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              children: [
                TextField(
                  key: const ValueKey('sign-in-username'),
                  controller: _username,
                  autocorrect: false,
                  enableSuggestions: false,
                  autofillHints: const [AutofillHints.username],
                  textInputAction: TextInputAction.next,
                  onSubmitted: (_) => _passwordFocus.requestFocus(),
                  decoration: const InputDecoration(labelText: 'Username', border: InputBorder.none),
                ),
                TextField(
                  key: const ValueKey('sign-in-password'),
                  controller: _password,
                  focusNode: _passwordFocus,
                  obscureText: !_shown,
                  autocorrect: false,
                  enableSuggestions: false,
                  autofillHints: const [AutofillHints.password],
                  textInputAction: TextInputAction.done,
                  onSubmitted: (_) => _submit(),
                  decoration: InputDecoration(
                    labelText: 'Password',
                    border: InputBorder.none,
                    suffixIcon: IconButton(
                      tooltip: _shown ? 'Hide password' : 'Show password',
                      icon: Icon(_shown ? CupertinoIcons.eye_slash : CupertinoIcons.eye, size: 20),
                      onPressed: () => setState(() => _shown = !_shown),
                    ),
                  ),
                ),
              ],
            ),
          ),
          if (error != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(32, 8, 32, 0),
              child: Row(children: [
                Icon(CupertinoIcons.xmark_circle, size: 16, color: Tokens.negative),
                const SizedBox(width: 6),
                Expanded(child: Text(error, key: const ValueKey('sign-in-error'), style: TextStyle(fontSize: 13, color: Tokens.negative))),
              ]),
            ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 24, 16, 0),
            child: FilledButton(
              key: const ValueKey('sign-in'),
              style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(50)),
              onPressed: _busy || wait != null ? null : _submit,
              child: Text(wait != null ? 'Try again in ${waitClock(wait)}' : _busy ? 'Signing in…' : 'Sign in'),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(32, 16, 32, 0),
            child: Text('Forgot it? Ask another board member to reset your password.', textAlign: TextAlign.center, style: TextStyle(fontSize: 13, color: Tokens.label2)),
          ),
          const OtherServer(),
        ],
      ),
    );
  }
}

/// Change a password (MU3-05 AC1). [required]: the board member signed in
/// with a temporary one (B10) and nothing else opens first (the gate keeps
/// the phone here); otherwise from Profile.
class ChangePasswordPage extends StatefulWidget {
  const ChangePasswordPage({super.key, this.required = false});

  final bool required;

  @override
  State<ChangePasswordPage> createState() => _ChangePasswordPageState();
}

class _ChangePasswordPageState extends State<ChangePasswordPage> {
  final _old = TextEditingController();
  final _new = TextEditingController();
  final _repeat = TextEditingController();
  String? _oldError;
  String? _newError;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    for (final c in [_old, _new, _repeat]) {
      c.addListener(() => setState(() {}));
    }
  }

  @override
  void dispose() {
    _old.dispose();
    _new.dispose();
    _repeat.dispose();
    super.dispose();
  }

  bool get _ok => _old.text.isNotEmpty && BoardMemberRules.password(_new.text) && _new.text == _repeat.text;

  Future<void> _save() async {
    if (!_ok || _busy) return;
    setState(() {
      _busy = true;
      _oldError = null;
      _newError = null;
    });
    final messenger = ScaffoldMessenger.of(context);
    final router = GoRouter.maybeOf(context);
    try {
      await context.read<AureliusRepository>().setPassword(newPassword: _new.text, oldPassword: _old.text);
      for (final c in [_old, _new, _repeat]) {
        c.clear();
      }
      messenger
        ..hideCurrentSnackBar()
        ..showSnackBar(SnackBar(content: Text(widget.required ? 'Password changed. Welcome.' : 'Password changed. Your other devices are signed out.')));
      if (!widget.required && mounted && (router?.canPop() ?? false)) router!.pop();
    } on ProtocolException catch (e) {
      if (!mounted) return;
      setState(() => switch (e.code) {
            ErrorCode.wrongPassword => _oldError = widget.required ? 'The temporary password is wrong.' : 'The current password is wrong.',
            ErrorCode.weakPassword => _newError = e.message.isEmpty ? BoardMemberRules.passwordRule : e.message,
            _ => _newError = e.shown,
          });
    } catch (e) {
      if (mounted) setState(() => _newError = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Widget _field(String key, String label, TextEditingController c, List<String> hints, {String? error}) => TextField(
        key: ValueKey(key),
        controller: c,
        obscureText: true,
        autocorrect: false,
        enableSuggestions: false,
        autofillHints: hints,
        decoration: InputDecoration(labelText: label, border: InputBorder.none, errorText: error),
      );

  @override
  Widget build(BuildContext context) {
    final problem = _new.text.isEmpty ? null : BoardMemberRules.passwordProblem(_new.text);
    final differ = _repeat.text.isNotEmpty && _repeat.text != _new.text;
    return LargeTitleScaffold(
      title: widget.required ? 'Choose a new password' : 'Change password',
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 8, 20, 16),
          child: Text(
            widget.required
                ? 'You signed in with a temporary password. Choose your own to continue.'
                : 'Your other devices are signed out; this phone stays signed in.',
            style: TextStyle(fontSize: 15, color: Tokens.label2),
          ),
        ),
        AutofillGroup(
          child: InsetGroup(padding: const EdgeInsets.symmetric(horizontal: 16), children: [
            _field('password-old', widget.required ? 'Temporary password' : 'Current password', _old, const [AutofillHints.password], error: _oldError),
            _field('password-new', 'New password', _new, const [AutofillHints.newPassword], error: _newError ?? problem),
            _field('password-repeat', 'Repeat new password', _repeat, const [AutofillHints.newPassword], error: differ ? 'The passwords differ.' : null),
          ]),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(32, 6, 32, 0),
          child: Text(BoardMemberRules.passwordRule, style: TextStyle(fontSize: 13, color: Tokens.label2)),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 24, 16, 0),
          child: FilledButton(
            key: const ValueKey('password-save'),
            style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(50)),
            onPressed: _ok && !_busy ? _save : null,
            child: Text(_busy ? 'Saving…' : 'Change password'),
          ),
        ),
        if (widget.required)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Center(
              child: TextButton(
                key: const ValueKey('password-sign-out'),
                onPressed: () => context.read<AureliusRepository>().signOut(),
                child: const Text('Sign out'),
              ),
            ),
          ),
      ],
    );
  }
}

/// A temporary password for another board member (B10, P-34): they change it
/// at their next sign-in, and all their sessions end now.
class TemporaryPasswordPage extends StatefulWidget {
  const TemporaryPasswordPage({super.key, required this.userId, required this.username});

  final String userId;
  final String username;

  @override
  State<TemporaryPasswordPage> createState() => _TemporaryPasswordPageState();
}

class _TemporaryPasswordPageState extends State<TemporaryPasswordPage> {
  final _password = TextEditingController();
  String? _error;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _password.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _password.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    if (!BoardMemberRules.password(_password.text) || _busy) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    final messenger = ScaffoldMessenger.of(context);
    final router = GoRouter.maybeOf(context);
    try {
      await context.read<AureliusRepository>().setPassword(newPassword: _password.text, userId: widget.userId);
      _password.clear();
      messenger
        ..hideCurrentSnackBar()
        ..showSnackBar(SnackBar(content: Text('${widget.username} is signed out everywhere and chooses a new password at the next sign-in.')));
      if (mounted && (router?.canPop() ?? false)) router!.pop();
    } on ProtocolException catch (e) {
      if (mounted) setState(() => _error = e.code == ErrorCode.weakPassword && e.message.isNotEmpty ? e.message : e.shown);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => LargeTitleScaffold(
        title: 'Temporary password',
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 8, 20, 16),
            child: Text(
              'For ${widget.username}. Give it to them yourself; they choose their own at the next sign-in. All their devices are signed out now.',
              style: TextStyle(fontSize: 15, color: Tokens.label2),
            ),
          ),
          InsetGroup(padding: const EdgeInsets.symmetric(horizontal: 16), children: [
            TextField(
              key: const ValueKey('temporary-password'),
              controller: _password,
              obscureText: true,
              autocorrect: false,
              enableSuggestions: false,
              autofillHints: const [AutofillHints.newPassword],
              decoration: InputDecoration(
                labelText: 'Temporary password',
                border: InputBorder.none,
                errorText: _error ?? (_password.text.isEmpty ? null : BoardMemberRules.passwordProblem(_password.text)),
              ),
            ),
          ]),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 24, 16, 0),
            child: FilledButton(
              key: const ValueKey('temporary-password-save'),
              style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(50)),
              onPressed: BoardMemberRules.password(_password.text) && !_busy ? _save : null,
              child: Text(_busy ? 'Saving…' : 'Set and sign out ${widget.username}'),
            ),
          ),
        ],
      );
}

/// "Signed in Oct 31 · active now" for a session row (ui-ontwerp §2).
String sessionLine(SessionView s, {DateTime? now}) {
  final at = now ?? DateTime.now();
  final since = DateTime.fromMillisecondsSinceEpoch(s.createdAtMs);
  final seen = DateTime.fromMillisecondsSinceEpoch(s.lastSeenMs);
  final idle = at.difference(seen);
  final active = s.current || idle.inMinutes < 2
      ? 'active now'
      : idle.inMinutes < 60
          ? 'active ${idle.inMinutes} min ago'
          : idle.inHours < 24
              ? 'active ${idle.inHours} h ago'
              : 'active ${DateFormat('MMM d').format(seen)}';
  return 'Signed in ${DateFormat('MMM d').format(since)} · $active';
}

/// Profile → Sessions (MU3-04): the devices signed in as a board
/// member, newest activity first; this phone is marked and signs out with
/// Sign out instead. Swipe left (or Revoke) ends another, after a
/// confirmation. [userId]: another board member's (AC4, one role).
class SessionsPage extends StatefulWidget {
  const SessionsPage({super.key, this.userId, this.username});

  final String? userId;
  final String? username;

  @override
  State<SessionsPage> createState() => _SessionsPageState();
}

class _SessionsPageState extends State<SessionsPage> {
  List<SessionView>? _sessions;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final snapshot = await context.read<AureliusRepository>().sessions(userId: widget.userId);
      if (mounted) {
        setState(() {
          _sessions = snapshot.sessions;
          _error = null;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = e is ProtocolException ? e.shown : '$e');
    }
  }

  Future<bool> _revoke(SessionView s) async {
    final repository = context.read<AureliusRepository>();
    final ok = await confirm(context, title: 'Sign out ${s.deviceName}?', body: 'It needs ${widget.username == null ? 'your' : "${widget.username}'s"} password to come back.', action: 'Revoke', danger: true);
    if (!ok || !mounted) return false;
    try {
      await repository.revokeSession(s.id);
      if (mounted) setState(() => _sessions = [...?_sessions?.where((x) => x.id != s.id)]);
      return true;
    } catch (e) {
      if (mounted) toast(context, e is ProtocolException ? e.shown : '$e');
      return false;
    }
  }

  Future<void> _revokeOthers() async {
    final others = [...?_sessions?.where((s) => !s.current)];
    if (others.isEmpty) return;
    final repository = context.read<AureliusRepository>();
    final ok = await confirm(
      context,
      title: widget.userId == null ? 'Sign out all other devices?' : 'Sign out ${widget.username ?? 'them'} everywhere?',
      body: 'They need the password to come back.',
      action: 'Sign out',
      danger: true,
    );
    if (!ok) return;
    try {
      for (final s in others) {
        await repository.revokeSession(s.id);
      }
    } catch (e) {
      if (mounted) toast(context, e is ProtocolException ? e.shown : '$e');
    }
    await _load();
  }

  @override
  Widget build(BuildContext context) {
    final sessions = _sessions;
    final me = context.select<ConnectionBloc, String?>((b) => b.state.user?.username);
    final who = widget.username ?? me ?? 'you';
    return LargeTitleScaffold(
      title: 'Sessions',
      onRefresh: _load,
      fill: sessions == null
          ? Center(
              child: _error == null
                  ? const CircularProgressIndicator()
                  : Padding(
                      padding: const EdgeInsets.all(24),
                      child: Column(mainAxisSize: MainAxisSize.min, children: [
                        Text(_error!, key: const ValueKey('sessions-error'), textAlign: TextAlign.center, style: TextStyle(color: Tokens.label2)),
                        TextButton(onPressed: _load, child: const Text('Try again')),
                      ]),
                    ),
            )
          : null,
      children: sessions == null
          ? null
          : [
              GroupHeader('Devices signed in as $who'),
              InsetGroup(
                dividerIndent: 56,
                children: [
                  if (sessions.isEmpty) const GroupRow(title: 'No devices are signed in.'),
                  for (final s in sessions)
                    s.current
                        ? GroupRow(
                            key: ValueKey('session-${s.id}'),
                            leading: const Icon(CupertinoIcons.device_phone_portrait),
                            title: s.deviceName,
                            subtitle: sessionLine(s),
                            value: 'This device',
                          )
                        : Dismissible(
                            key: ValueKey('session-${s.id}'),
                            direction: DismissDirection.endToStart,
                            confirmDismiss: (_) => _revoke(s),
                            background: Container(
                              color: Tokens.negative,
                              alignment: Alignment.centerRight,
                              padding: const EdgeInsets.only(right: 20),
                              child: const Text('Revoke', style: TextStyle(color: Colors.white, fontWeight: FontWeight.w600)),
                            ),
                            child: GroupRow(
                              leading: Icon(s.clientId == null ? CupertinoIcons.desktopcomputer : CupertinoIcons.device_phone_portrait),
                              title: s.deviceName,
                              subtitle: sessionLine(s),
                              trailing: TextButton(key: ValueKey('revoke-${s.id}'), onPressed: () => _revoke(s), child: const Text('Revoke')),
                            ),
                          ),
                ],
              ),
              if (sessions.any((s) => !s.current))
                Padding(
                  padding: const EdgeInsets.only(top: 20),
                  child: InsetGroup(children: [
                    GroupRow(
                      key: const ValueKey('sessions-revoke-others'),
                      title: widget.userId == null ? 'Sign out all other devices' : 'Sign out all devices',
                      titleStyle: TextStyle(fontSize: 17, color: Tokens.negative),
                      chevron: false,
                      onTap: _revokeOthers,
                    ),
                  ]),
                ),
            ],
    );
  }
}
