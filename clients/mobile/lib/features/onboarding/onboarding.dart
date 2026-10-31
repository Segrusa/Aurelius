import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart' show Monogram, toast;
import '../../data/aurelius_repository.dart';
import '../../data/protocol/protocol.dart';
import '../connection/connection_bloc.dart';

/// Where the app goes for [step] while at [location] (impactanalyse §4b.4):
/// without an active board member only the onboarding and the server choice
/// are reachable (MU1-20 AC1, MU1-21 AC2); with one, the onboarding is left.
/// Null: stay. Before the first hello nothing is decided.
String? boardMemberRedirect(BoardMemberStep step, String location) {
  final onboarding = location == '/onboarding' || location.startsWith('/onboarding/');
  final serverChoice = location == '/hosts' || location == '/pair';
  return switch (step) {
    BoardMemberStep.unknown => null,
    BoardMemberStep.createFirst || BoardMemberStep.choose => onboarding || serverChoice ? null : '/onboarding',
    BoardMemberStep.ready => onboarding ? '/home' : null,
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

/// `/onboarding`: the first board member, or "Who's working here?".
class OnboardingPage extends StatelessWidget {
  const OnboardingPage({super.key});

  @override
  Widget build(BuildContext context) {
    final step = context.select<ConnectionBloc, BoardMemberStep>((b) => b.state.boardMemberStep);
    return switch (step) {
      BoardMemberStep.createFirst => const NewBoardMemberPage(first: true),
      _ => const ChooseBoardMemberPage(),
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
      // Without a board member this phone became the new one; the gate opens the app.
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
          NotSentException(sent: true) => 'The connection dropped. If $username is in the list, choose it.',
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
        if (widget.first) const _OtherServer(),
      ],
    );
  }
}

/// "Choose another server", so a phone on the wrong server is not stuck.
class _OtherServer extends StatelessWidget {
  const _OtherServer();

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

/// "Who's working here?" (MU1-21 AC2): the board members and "New board
/// member"; a choice asks once for the password (B8). [switching]: opened
/// from More with a board member already active.
class ChooseBoardMemberPage extends StatefulWidget {
  const ChooseBoardMemberPage({super.key, this.switching = false});

  final bool switching;

  @override
  State<ChooseBoardMemberPage> createState() => _ChooseBoardMemberPageState();
}

class _ChooseBoardMemberPageState extends State<ChooseBoardMemberPage> {
  List<UserView>? _members;
  String? _error;
  StreamSubscription<Object?>? _changes;

  @override
  void initState() {
    super.initState();
    final repository = context.read<AureliusRepository>();
    _changes = repository.boardMemberChanges.listen((_) => _load());
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
      if (mounted) {
        setState(() {
          _members = members;
          _error = null;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    }
  }

  Future<void> _choose(UserView member) async {
    final chosen = await showPasswordSheet(context, member, switching: widget.switching);
    if (chosen == null || !mounted) return;
    if (widget.switching && Navigator.of(context).canPop()) Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final link = context.watch<ConnectionBloc>().state;
    final members = _members;
    return LargeTitleScaffold(
      title: "Who's working here?",
      onRefresh: _load,
      fill: members == null
          ? Center(
              child: _error == null
                  ? const CircularProgressIndicator()
                  : Padding(
                      padding: const EdgeInsets.all(24),
                      child: Column(mainAxisSize: MainAxisSize.min, children: [
                        Text(_error!, textAlign: TextAlign.center, style: TextStyle(color: Tokens.label2)),
                        TextButton(onPressed: _load, child: const Text('Try again')),
                      ]),
                    ),
            )
          : null,
      children: members == null
          ? null
          : [
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 8, 20, 16),
                child: Text('Choose your board member on this server.', style: TextStyle(fontSize: 15, color: Tokens.label2)),
              ),
              InsetGroup(
                dividerIndent: 66,
                children: [
                  for (final m in members)
                    GroupRow(
                      key: ValueKey('board-member-${m.id}'),
                      leading: Monogram(_monogram(m.username), active: m.id == link.user?.id),
                      title: m.username,
                      value: m.id == link.user?.id ? 'you' : null,
                      chevron: true,
                      onTap: m.id == link.user?.id ? null : () => _choose(m),
                    ),
                ],
              ),
              const SizedBox(height: 20),
              InsetGroup(children: [
                GroupRow(
                  key: const ValueKey('board-member-new'),
                  leading: Icon(CupertinoIcons.add, color: Tokens.accent),
                  title: 'New board member',
                  titleStyle: TextStyle(fontSize: 17, color: Tokens.accent),
                  onTap: () => context.push(widget.switching ? '/settings/board-members/new' : '/onboarding/new'),
                ),
              ]),
              if (!widget.switching) const _OtherServer(),
            ],
    );
  }
}

/// The one password check for [member] (B8). Returns the chosen username, or
/// null when cancelled. The password lives only in the sheet's field.
Future<String?> showPasswordSheet(BuildContext context, UserView member, {bool switching = false}) {
  final repository = context.read<AureliusRepository>();
  return showModalBottomSheet<String>(
    context: context,
    isScrollControlled: true,
    builder: (sheet) => RepositoryProvider.value(value: repository, child: _PasswordSheet(member, switching: switching)),
  );
}

class _PasswordSheet extends StatefulWidget {
  const _PasswordSheet(this.member, {required this.switching});

  final UserView member;
  final bool switching;

  @override
  State<_PasswordSheet> createState() => _PasswordSheetState();
}

class _PasswordSheetState extends State<_PasswordSheet> {
  final _password = TextEditingController();
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _password.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    if (_password.text.isEmpty || _busy) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    final messenger = ScaffoldMessenger.of(context);
    try {
      final user = await context.read<AureliusRepository>().useBoardMember(widget.member.id, _password.text);
      _password.clear();
      // The gate may close this sheet with the onboarding: say it here.
      messenger
        ..hideCurrentSnackBar()
        ..showSnackBar(SnackBar(content: Text(widget.switching ? 'You are ${user.username} now.' : 'Welcome, ${user.username}.')));
      if (mounted) Navigator.of(context).pop(user.username);
    } on ProtocolException catch (e) {
      if (!mounted) return;
      setState(() => _error = switch (e.code) {
            ErrorCode.wrongPassword => 'Wrong password.',
            ErrorCode.tooManyAttempts => 'Too many tries. Wait a minute and try again.',
            _ => e.shown,
          });
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
        child: SafeArea(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(20, 20, 20, 16),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(children: [
                  Monogram(_monogram(widget.member.username)),
                  const SizedBox(width: 12),
                  Expanded(child: Text(widget.member.username, style: Tokens.title(size: 22))),
                ]),
                const SizedBox(height: 12),
                AutofillGroup(
                  child: TextField(
                    key: const ValueKey('board-member-check-password'),
                    controller: _password,
                    autofocus: true,
                    obscureText: true,
                    enableSuggestions: false,
                    autocorrect: false,
                    autofillHints: const [AutofillHints.password],
                    textInputAction: TextInputAction.done,
                    onSubmitted: (_) => _submit(),
                    decoration: InputDecoration(labelText: 'Password', errorText: _error),
                  ),
                ),
                const SizedBox(height: 8),
                Text('This device remembers you. Full sign-in comes later.', style: TextStyle(fontSize: 13, color: Tokens.label2)),
                const SizedBox(height: 16),
                FilledButton(
                  key: const ValueKey('board-member-check'),
                  style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(50)),
                  onPressed: _busy ? null : _submit,
                  child: Text(_busy ? 'Checking…' : 'Continue'),
                ),
              ],
            ),
          ),
        ),
      );
}

/// More → Settings → Board members (MU1-22): who they are, "you", and New
/// board member. No delete or password reset in Fase 1 (OV-14).
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
                value: m.id == me ? 'you' : null,
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

/// The active board member at the top of More (MU1-21 AC4); › switches.
class ActiveBoardMemberRow extends StatelessWidget {
  const ActiveBoardMemberRow({super.key});

  @override
  Widget build(BuildContext context) {
    final user = context.select<ConnectionBloc, UserView?>((b) => b.state.user);
    if (user == null) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(top: 20),
      child: InsetGroup(children: [
        GroupRow(
          key: const ValueKey('more-board-member'),
          leading: Monogram(_monogram(user.username), active: true),
          title: user.username,
          subtitle: 'Switch board member',
          chevron: true,
          onTap: () => context.push('/switch-board-member'),
        ),
      ]),
    );
  }
}
