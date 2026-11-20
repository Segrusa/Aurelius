import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/dialogs.dart' show confirm;
import '../../core/widgets/widgets.dart' show Monogram, toast;
import '../account/account.dart';
import '../../data/aurelius_repository.dart';
import '../../data/tickets.dart' show openResponsibleCount, ticketResponsibleFeature;
import '../../data/protocol/protocol.dart';
import '../connection/connection_bloc.dart';

/// Where the app goes for [step] while at [location] (MU3-02 AC1, MU3-06):
/// without a session only the onboarding (the first board member or the
/// sign-in screen) and the server choice are reachable; a temporary password
/// keeps the phone on its change (B10); signed in, the onboarding is left.
/// Null: stay. Before the first hello nothing is decided.
String? boardMemberRedirect(BoardMemberStep step, String location) {
  final onboarding = location == '/onboarding' || location.startsWith('/onboarding/');
  final serverChoice = location == '/hosts' || location == '/pair';
  return switch (step) {
    BoardMemberStep.unknown => null,
    BoardMemberStep.createFirst || BoardMemberStep.signIn => onboarding || serverChoice ? null : '/onboarding',
    BoardMemberStep.changePassword => location == '/onboarding' ? null : '/onboarding',
    BoardMemberStep.ready => onboarding ? '/chat' : null,
  };
}

/// OV-12 as a help in the client; the server decides.
abstract final class BoardMemberRules {
  static final _username = RegExp(r'^[A-Za-z0-9._-]{3,32}$');
  static const usernameRule = '3–32 characters: letters, digits, . _ -';
  static const passwordRule = 'At least 12 characters';

  static bool username(String text) => _username.hasMatch(text.trim());
  /// As the server counts: characters (not UTF-16 units), 12 to 1024, not only spaces.
  static bool password(String text) => passwordProblem(text) == null;

  /// What is wrong with [text], in the server's words; null when it passes.
  static String? passwordProblem(String text) {
    final count = text.runes.length;
    if (count > 0 && text.trim().isEmpty) return 'A password of only spaces is too weak.';
    if (count < 12) return passwordRule;
    if (count > 1024) return 'A password has at most 1024 characters.';
    return null;
  }
}

String _monogram(String name) => name.isEmpty ? '?' : String.fromCharCode(name.runes.first).toUpperCase();

/// `/onboarding`: the first board member, the sign-in screen, or the change
/// of a temporary password.
class OnboardingPage extends StatelessWidget {
  const OnboardingPage({super.key});

  @override
  Widget build(BuildContext context) {
    final step = context.select<ConnectionBloc, BoardMemberStep>((b) => b.state.boardMemberStep);
    return switch (step) {
      BoardMemberStep.createFirst => const NewBoardMemberPage(first: true),
      BoardMemberStep.changePassword => const ChangePasswordPage(required: true),
      _ => const SignInPage(),
    };
  }
}

/// "Create the first board member" (MU1-20) or "New board member" (MU1-21
/// AC2, MU1-22): username, password and its repeat, with the rules below
/// them. The password stays in this page's fields only: never logged,
/// kept or shown in a toast.
class NewBoardMemberPage extends StatefulWidget {
  const NewBoardMemberPage({super.key, this.first = false});

  /// The server has no board members yet: nothing else is reachable.
  final bool first;

  @override
  State<NewBoardMemberPage> createState() => _NewBoardMemberPageState();
}

class _NewBoardMemberPageState extends State<NewBoardMemberPage> {
  final _username = TextEditingController();
  final _password = TextEditingController();
  final _repeat = TextEditingController();
  final _passwordFocus = FocusNode();
  final _repeatFocus = FocusNode();
  final _touched = <String>{};
  String? _usernameError;
  String? _passwordError;
  bool _busy = false;
  bool _shown = false;

  @override
  void initState() {
    super.initState();
    for (final c in [_username, _password, _repeat]) {
      c.addListener(() => setState(() {}));
    }
  }

  @override
  void dispose() {
    _username.dispose();
    _password.dispose();
    _repeat.dispose();
    _passwordFocus.dispose();
    _repeatFocus.dispose();
    super.dispose();
  }

  bool get _usernameOk => BoardMemberRules.username(_username.text);
  bool get _passwordOk => BoardMemberRules.password(_password.text);
  bool get _repeatOk => _repeat.text.isNotEmpty && _repeat.text == _password.text;

  String? _shownError(String field, bool ok, String rule, String? server) {
    if (server != null) return server;
    if (!_touched.contains(field) || ok) return null;
    return rule;
  }

  Future<void> _create() async {
    setState(() => _touched.addAll(['username', 'password', 'repeat']));
    if (!_usernameOk || !_passwordOk || !_repeatOk || _busy) return;
    final repository = context.read<AureliusRepository>();
    final messenger = ScaffoldMessenger.of(context);
    final navigator = Navigator.of(context);
    final hadMember = repository.linkState.user != null;
    final username = _username.text.trim();
    setState(() {
      _busy = true;
      _usernameError = null;
      _passwordError = null;
    });
    try {
      await repository.createBoardMember(username, _password.text);
      _password.clear();
      _repeat.clear();
      // The first board member signs this phone in; the gate opens the app.
      messenger
        ..hideCurrentSnackBar()
        ..showSnackBar(SnackBar(content: Text(hadMember ? '$username is a board member now.' : 'Welcome, $username.')));
      if (hadMember && mounted && navigator.canPop()) navigator.pop();
    } on ProtocolException catch (e) {
      if (!mounted) return;
      setState(() {
        switch (e.code) {
          case ErrorCode.usernameTaken:
            _usernameError = 'This username is taken.';
          case ErrorCode.invalidUsername:
            _usernameError = BoardMemberRules.usernameRule;
          case ErrorCode.weakPassword:
            // The server says which rule (too short, too long, only spaces).
            _passwordError = e.message.isEmpty ? BoardMemberRules.passwordRule : e.message;
          default:
            toast(context, e.shown);
        }
      });
    } catch (e) {
      if (mounted) {
        toast(context, switch (e) {
          // Sent, but the answer never came: the PC may have made it (Trace M3a m2).
          NotSentException(sent: true) => 'The connection dropped. If $username was made, sign in as $username.',
          NotSentException() => 'Not sent. The PC was not reachable.',
          _ => '$e',
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Widget _rule(String text, bool ok) => Padding(
        padding: const EdgeInsets.fromLTRB(32, 6, 32, 0),
        child: Row(children: [
          Expanded(child: Text(text, style: TextStyle(fontSize: 13, color: Tokens.label2))),
          if (ok) Icon(CupertinoIcons.checkmark_alt, size: 16, color: Tokens.accent, semanticLabel: 'OK'),
        ]),
      );

  Widget _error(String? text) => text == null
      ? const SizedBox.shrink()
      : Padding(
          padding: const EdgeInsets.fromLTRB(32, 4, 32, 0),
          child: Text(text, style: TextStyle(fontSize: 13, color: Tokens.negative)),
        );

  Widget _field({
    required Key key,
    required String label,
    required TextEditingController controller,
    required String field,
    FocusNode? focus,
    bool obscure = false,
    Iterable<String>? autofill,
    TextInputAction action = TextInputAction.next,
    VoidCallback? onSubmitted,
    Widget? suffix,
  }) =>
      Focus(
        onFocusChange: (on) {
          if (!on && controller.text.isNotEmpty) setState(() => _touched.add(field));
        },
        child: TextField(
          key: key,
          controller: controller,
          focusNode: focus,
          obscureText: obscure,
          enableSuggestions: !obscure,
          autocorrect: false,
          autofillHints: autofill,
          textInputAction: action,
          onSubmitted: (_) => onSubmitted?.call(),
          decoration: InputDecoration(labelText: label, border: InputBorder.none, suffixIcon: suffix),
        ),
      );

  @override
  Widget build(BuildContext context) {
    final usernameError = _shownError('username', _usernameOk, BoardMemberRules.usernameRule, _usernameError);
    final passwordError = _shownError('password', _passwordOk, BoardMemberRules.passwordProblem(_password.text) ?? BoardMemberRules.passwordRule, _passwordError);
    final repeatError = _shownError('repeat', _repeatOk, 'The passwords differ.', null);
    return LargeTitleScaffold(
      title: widget.first ? 'Create the first board member' : 'New board member',
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 8, 20, 16),
          child: Text(
            widget.first
                ? 'This server has no board members yet. Board members create tickets and decide in review.'
                : 'A board member creates tickets and decides in review.',
            style: TextStyle(fontSize: 15, color: Tokens.label2),
          ),
        ),
        AutofillGroup(
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            InsetGroup(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              children: [
                _field(
                  key: const ValueKey('board-member-username'),
                  label: 'Username',
                  controller: _username,
                  field: 'username',
                  autofill: const [AutofillHints.username],
                  onSubmitted: _passwordFocus.requestFocus,
                ),
                _field(
                  key: const ValueKey('board-member-password'),
                  label: 'Password',
                  controller: _password,
                  field: 'password',
                  focus: _passwordFocus,
                  obscure: !_shown,
                  autofill: const [AutofillHints.newPassword],
                  onSubmitted: _repeatFocus.requestFocus,
                  suffix: IconButton(
                    tooltip: _shown ? 'Hide password' : 'Show password',
                    icon: Icon(_shown ? CupertinoIcons.eye_slash : CupertinoIcons.eye, size: 20),
                    onPressed: () => setState(() => _shown = !_shown),
                  ),
                ),
                _field(
                  key: const ValueKey('board-member-repeat'),
                  label: 'Repeat password',
                  controller: _repeat,
                  field: 'repeat',
                  focus: _repeatFocus,
                  obscure: !_shown,
                  autofill: const [AutofillHints.newPassword],
                  action: TextInputAction.done,
                  onSubmitted: _create,
                ),
              ],
            ),
          ]),
        ),
        _rule(BoardMemberRules.usernameRule, _usernameOk),
        _error(usernameError),
        _rule(BoardMemberRules.passwordRule, _passwordOk),
        _error(passwordError),
        _error(repeatError),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 24, 16, 0),
          child: FilledButton(
            key: const ValueKey('board-member-create'),
            style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(50)),
            onPressed: _busy ? null : _create,
            child: Text(_busy ? 'Creating…' : 'Create board member'),
          ),
        ),
        if (widget.first) const OtherServer(),
      ],
    );
  }
}

/// "Choose another server", so a phone on the wrong server is not stuck.
class OtherServer extends StatelessWidget {
  const OtherServer({super.key});

  @override
  Widget build(BuildContext context) {
    final link = context.watch<ConnectionBloc>().state;
    final host = context.read<AureliusRepository>().host;
    return Padding(
      padding: const EdgeInsets.only(top: 20),
      child: Column(children: [
        Text('${link.isOnline ? 'Connected to' : 'Not connected to'} ${host.name}', style: TextStyle(fontSize: 13, color: Tokens.label2)),
        TextButton(key: const ValueKey('onboarding-other-server'), onPressed: () => context.push('/hosts'), child: const Text('Choose another server')),
      ]),
    );
  }
}

/// Profile → Board members (MU1-22, MU3-04 AC4, MU3-05): who they
/// are, "you", disabled ones, and per board member their sessions, a
/// temporary password (B10) and Disable. One role: anyone may do this.
class BoardMembersPage extends StatefulWidget {
  const BoardMembersPage({super.key});

  @override
  State<BoardMembersPage> createState() => _BoardMembersPageState();
}

class _BoardMembersPageState extends State<BoardMembersPage> {
  List<UserView> _members = const [];
  StreamSubscription<Object?>? _changes;

  @override
  void initState() {
    super.initState();
    _changes = context.read<AureliusRepository>().boardMemberChanges.listen((_) => _load());
    _load();
  }

  @override
  void dispose() {
    _changes?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final members = await context.read<AureliusRepository>().boardMembers();
      if (mounted) setState(() => _members = members);
    } catch (e) {
      if (mounted) toast(context, '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final me = context.select<ConnectionBloc, String?>((b) => b.state.user?.id);
    return LargeTitleScaffold(
      title: 'Board members',
      onRefresh: _load,
      children: [
        const SizedBox(height: 20),
        InsetGroup(
          dividerIndent: 66,
          children: [
            for (final m in _members)
              GroupRow(
                key: ValueKey('board-members-${m.id}'),
                leading: Monogram(_monogram(m.username), active: m.id == me),
                title: m.username,
                subtitle: m.disabledAtMs != null ? 'Disabled · cannot sign in' : null,
                value: m.id == me ? 'you' : null,
                chevron: true,
                onTap: () => m.id == me ? context.go('/profile') : showBoardMemberActions(context, m, _members),
              ),
          ],
        ),
        const SizedBox(height: 20),
        InsetGroup(children: [
          GroupRow(
            key: const ValueKey('board-members-new'),
            leading: Icon(CupertinoIcons.add, color: Tokens.accent),
            title: 'New board member',
            titleStyle: TextStyle(fontSize: 17, color: Tokens.accent),
            onTap: () => context.push('/settings/board-members/new'),
          ),
        ]),
      ],
    );
  }
}

/// What a board member can do for another (one role): their sessions, a
/// temporary password (B10, P-34) and Disable (MU3-05 AC3/AC4).
Future<void> showBoardMemberActions(BuildContext context, UserView member, List<UserView> all) async {
  final disabled = member.disabledAtMs != null;
  final choice = await showModalBottomSheet<String>(
    context: context,
    builder: (sheet) => SafeArea(
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        ListTile(title: Text(member.username, style: Tokens.title(size: 20)), subtitle: disabled ? const Text('Disabled') : null),
        ListTile(key: const ValueKey('member-sessions'), leading: const Icon(CupertinoIcons.device_phone_portrait), title: const Text('Sessions'), onTap: () => Navigator.pop(sheet, 'sessions')),
        if (!disabled) ...[
          ListTile(key: const ValueKey('member-reset'), leading: const Icon(CupertinoIcons.lock_rotation), title: const Text('Set a temporary password'), onTap: () => Navigator.pop(sheet, 'reset')),
          ListTile(
            key: const ValueKey('member-disable'),
            leading: Icon(CupertinoIcons.person_crop_circle_badge_xmark, color: Tokens.negative),
            title: Text('Disable ${member.username}', style: TextStyle(color: Tokens.negative)),
            onTap: () => Navigator.pop(sheet, 'disable'),
          ),
        ],
      ]),
    ),
  );
  if (choice == null || !context.mounted) return;
  switch (choice) {
    case 'sessions':
      await context.push('/settings/board-members/${member.id}/sessions?name=${Uri.encodeQueryComponent(member.username)}');
    case 'reset':
      await context.push('/settings/board-members/${member.id}/password?name=${Uri.encodeQueryComponent(member.username)}');
    case 'disable':
      await disableBoardMember(context, member);
  }
}

/// Disable [member] after a confirmation (MU3-05 AC3); the last active board
/// member is refused by the server (AC4) and said so.
Future<bool> disableBoardMember(BuildContext context, UserView member) async {
  final repository = context.read<AureliusRepository>();
  // Nothing clears by itself (P-78): the name stays on those tickets, so it says how many.
  var responsibleFor = 0;
  if (repository.linkState.has(ticketResponsibleFeature)) {
    try {
      responsibleFor = openResponsibleCount(await repository.board(), member.id);
    } on Object {
      // The confirmation goes on without the count.
    }
    if (!context.mounted) return false;
  }
  final ok = await confirm(
    context,
    title: 'Disable ${member.username}?',
    body: '${member.username} is signed out on every device and cannot sign in. Their Claude login is disconnected. Their tickets keep them as creator.'
        '${responsibleFor == 0 ? '' : '\n\n${member.username} is responsible for $responsibleFor open ticket${responsibleFor == 1 ? '' : 's'}. They keep that name until someone changes it.'}',
    action: 'Disable',
    danger: true,
  );
  if (!ok || !context.mounted) return false;
  try {
    await repository.disableBoardMember(member.id);
    if (context.mounted) toast(context, '${member.username} is disabled.');
    return true;
  } on ProtocolException catch (e) {
    if (context.mounted) toast(context, e.code == ErrorCode.lastBoardMember ? 'The last active board member cannot be disabled.' : e.shown);
  } catch (e) {
    if (context.mounted) toast(context, '$e');
  }
  return false;
}
