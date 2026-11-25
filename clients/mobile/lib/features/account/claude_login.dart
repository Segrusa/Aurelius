import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart' show confirm;
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart' show AttentionCard, CopyButton, toast;
import '../../data/aurelius_repository.dart';
import '../../data/protocol/protocol.dart';
import '../connection/connection_bloc.dart';

// The board member's own Claude login (MU3-09, ui-ontwerp Fase 3 §4, Sentinel
// B1/B3). The phone shows only what `claude auth login` asks: the Anthropic
// sign-in page opens in the system browser (never a WebView, C3) and the code
// Claude shows goes back once with `claude_login.input`. That code lives in
// the sheet's field until it is sent: never logged, kept, put in a toast or
// in an error (C5–C8). Aurelius never asks for a Claude password or token.

/// Opens Anthropic's sign-in page in the phone's browser (C3); tests replace it.
Future<bool> Function(Uri uri) openClaudeSignIn = (uri) => launchUrl(uri, mode: LaunchMode.externalApplication);

/// What a code may look like before it goes to the server (C7): 1–512 of
/// `A-Z a-z 0-9 . _ ~ # -`, nothing else (no spaces, no line breaks).
final claudeCodeShape = RegExp(r'^[A-Za-z0-9._~#-]{1,512}$');

/// The residual risk of D5 in Sentinel's words (SEC2 m8), said where the login is connected.
const claudeLoginRisk =
    "Your Claude login is kept on this server. Other board members' agents could read your Claude login's files: their file tools are blocked from it, but a command they run is not. Connect only if you trust the work that runs on this server.";

/// After Disconnect (SEC2 m4): the server forgets the login, Anthropic keeps its session.
const claudeRevokeAtAnthropic = 'To end it at Anthropic too, revoke the session at claude.ai → Settings.';

/// A connected login whose /usage has not been read yet (QA3 M-m1).
const claudeUsageNotChecked = 'Usage not checked yet.';

/// [usage] only when it is of [me]'s own login (B12, R3 m3); the server
/// already answers per session, this is the phone's own check.
SubscriptionUsage? ownUsage(SubscriptionUsage? usage, String? me) => usage == null || usage.loginUser != me ? null : usage;

/// What [SubscriptionUsage] says about the own login (ui-ontwerp §4).
enum ClaudeUsageState { none, limits, limitReached, noSubscription, error }

/// A `/usage` without a "% used" line is an API login: never 0 % (MU3-09 AC5).
ClaudeUsageState claudeUsageState(SubscriptionUsage? usage) {
  if (usage == null || usage.fetchedAtMs == 0) return ClaudeUsageState.none;
  if (usage.error != null) return ClaudeUsageState.error;
  if (usage.limits.isEmpty) return usage.raw.trim().isEmpty ? ClaudeUsageState.none : ClaudeUsageState.noSubscription;
  return usage.limits.any((l) => l.percent >= 100) ? ClaudeUsageState.limitReached : ClaudeUsageState.limits;
}

/// "Limit reached until 4pm (UTC)." with the reset as Claude says it.
String claudeLimitReached(SubscriptionUsage usage) {
  final full = usage.limits.where((l) => l.percent >= 100).toList();
  final resets = full.map((l) => l.resets).whereType<String>().toList();
  return resets.isEmpty ? 'Limit reached.' : 'Limit reached until ${resets.last}.';
}

const claudeNoSubscription = 'This login has no Claude subscription (API usage only).';

/// The account rule of B1: one subscription or seat per real person.
const claudeLoginOwnAccount = 'Use your own Claude account: one subscription or Team seat per person, never shared.';

/// The terminal route (P-32, B1-4), for whoever works on the server itself.
String claudeLoginCommand(String userId) => 'CLAUDE_CONFIG_DIR=<Aurelius data>/logins/$userId claude auth login';

/// The short status for a row ("Connected", "Not connected", "Expired").
String? claudeLoginLine(ClaudeLoginView? login) => switch (login?.status) {
      null => null,
      ClaudeLoginStatus.linked => 'Connected',
      ClaudeLoginStatus.missing => 'Not connected',
      ClaudeLoginStatus.expired => 'Expired',
    };

/// The own login needs an action before tickets can run on it: with a board
/// member, a login that is not linked or not read yet counts as missing (R3 m5).
bool claudeLoginMissing(LinkState link) => link.user != null && link.claudeLogin?.status != ClaudeLoginStatus.linked;

/// "max" → "Max".
String _plan(String plan) => plan.isEmpty ? plan : plan[0].toUpperCase() + plan.substring(1);

/// Profile → Claude login (MU3-09 AC1/AC2/AC4, ui-ontwerp §4): the
/// status with words and a shape, never colour alone, and its actions.
class ClaudeLoginPage extends StatefulWidget {
  const ClaudeLoginPage({super.key});

  @override
  State<ClaudeLoginPage> createState() => _ClaudeLoginPageState();
}

class _ClaudeLoginPageState extends State<ClaudeLoginPage> {
  String? _error;
  bool _busy = false;
  SubscriptionUsage? _usage;

  /// Disconnected here: the note about Anthropic's session stays on the page.
  bool _disconnected = false;

  @override
  void initState() {
    super.initState();
    _check(quiet: true);
  }

  Future<void> _check({bool quiet = false}) async {
    final repository = context.read<AureliusRepository>();
    try {
      final login = await repository.claudeLogin();
      if (mounted) setState(() => _error = null);
      if (login?.status == ClaudeLoginStatus.linked) await _loadUsage(repository, refresh: !quiet);
    } catch (e) {
      if (mounted && !quiet) setState(() => _error = e is ProtocolException ? e.shown : '$e');
    }
  }

  /// `/usage` of the session's own login (B12); the first answer may be
  /// empty while the server fetches it, so it asks once more after that.
  /// A connected login whose kept /usage is empty or from before it was
  /// connected is read fresh once (QA3 M-m1).
  Future<void> _loadUsage(AureliusRepository repository, {bool refresh = false}) async {
    try {
      final me = repository.linkState.user?.id;
      var usage = ownUsage(await repository.subscription(refresh: refresh), me);
      if (usage != null && usage.fetchedAtMs == 0 && !refresh) {
        await Future<void>.delayed(usageRetry);
        if (!mounted) return;
        usage = ownUsage(await repository.subscription(), me);
      }
      if (!refresh && _unread(usage)) {
        if (!mounted) return;
        usage = ownUsage(await repository.subscription(refresh: true), me);
      }
      if (mounted) setState(() => _usage = usage);
    } catch (_) {
      // Usage is extra: the status above stays.
    }
  }

  /// Nothing usable for a connected login: none, or the server's "connect
  /// first" from before it was connected.
  static bool _unread(SubscriptionUsage? usage) => switch (claudeUsageState(usage)) {
        ClaudeUsageState.none => true,
        ClaudeUsageState.error => usage!.error!.startsWith('Connect your Claude login'),
        _ => false,
      };

  Future<void> _connect() async {
    final connected = await showConnectSheet(context);
    if (connected && mounted) toast(context, 'Your Claude login is connected.');
  }

  Future<void> _disconnect() async {
    final repository = context.read<AureliusRepository>();
    final ok = await confirm(
      context,
      title: 'Disconnect your Claude login?',
      body: 'Aurelius signs it out on the server and removes its folder. Your tickets wait until you connect again. $claudeRevokeAtAnthropic',
      action: 'Disconnect',
      danger: true,
    );
    if (!ok || !mounted) return;
    setState(() => _busy = true);
    try {
      await repository.unlinkClaudeLogin();
      await _check(quiet: true);
      if (mounted) {
        setState(() {
          _usage = null;
          _disconnected = true;
        });
        toast(context, 'Your Claude login is disconnected. $claudeRevokeAtAnthropic');
      }
    } catch (e) {
      if (mounted) toast(context, e is ProtocolException ? e.shown : '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final login = context.select<ConnectionBloc, ClaudeLoginView?>((b) => b.state.claudeLogin);
    final (icon, color, line) = switch (login?.status) {
      ClaudeLoginStatus.linked => (CupertinoIcons.checkmark_circle_fill, Tokens.accent, 'Connected'),
      ClaudeLoginStatus.expired => (CupertinoIcons.xmark_circle, Tokens.negative, 'Expired. Claude asked to sign in again.'),
      _ => (CupertinoIcons.circle, Tokens.label2, 'Not connected. Your tickets wait until you connect.'),
    };
    final linked = login?.status == ClaudeLoginStatus.linked;
    final details = [if (login?.email != null) login!.email!, if (login?.plan != null) _plan(login!.plan!)].join(' · ');
    final usageState = claudeUsageState(_usage);
    return BlocListener<ConnectionBloc, LinkState>(
      // Connected just now (here or elsewhere): its usage follows.
      listenWhen: (a, b) => a.claudeLogin?.status != ClaudeLoginStatus.linked && b.claudeLogin?.status == ClaudeLoginStatus.linked,
      listener: (context, _) => _loadUsage(context.read<AureliusRepository>()),
      child: LargeTitleScaffold(
        title: 'Claude login',
        onRefresh: _check,
        children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 8, 20, 16),
          child: Text('Tickets you create run on your own Claude subscription.', style: TextStyle(fontSize: 15, color: Tokens.label2)),
        ),
        InsetGroup(children: [
          GroupRow(
            key: const ValueKey('claude-status'),
            leading: Icon(icon, color: color, semanticLabel: claudeLoginLine(login) ?? 'Not connected'),
            title: line,
            titleLines: 3,
            subtitle: linked && details.isNotEmpty ? details : null,
          ),
          if (login?.linkId != null)
            const GroupRow(leading: Icon(CupertinoIcons.hourglass), title: 'A Connect is running.', subtitle: 'Finish it where it started, or start again here.'),
          if (linked && usageState == ClaudeUsageState.limitReached)
            GroupRow(key: const ValueKey('claude-limit'), leading: const Icon(CupertinoIcons.pause_circle, semanticLabel: 'Limit reached'), title: claudeLimitReached(_usage!), titleLines: 2, subtitle: 'Your tickets go on by themselves after the reset.'),
          if (linked && usageState == ClaudeUsageState.noSubscription)
            GroupRow(key: const ValueKey('claude-no-subscription'), leading: Icon(CupertinoIcons.exclamationmark_triangle, color: Tokens.warning, semanticLabel: 'No subscription'), title: claudeNoSubscription, titleLines: 3),
          if (linked && usageState == ClaudeUsageState.error && !_unread(_usage))
            GroupRow(key: const ValueKey('claude-usage-error'), leading: const Icon(CupertinoIcons.info), title: _usage!.error!, titleLines: 3),
          if (linked && _unread(_usage))
            const GroupRow(key: ValueKey('claude-usage-unchecked'), leading: Icon(CupertinoIcons.info), title: claudeUsageNotChecked, subtitle: 'Check now reads it.'),
          if (!linked && _disconnected)
            const GroupRow(key: ValueKey('claude-revoke-note'), leading: Icon(CupertinoIcons.info), title: claudeRevokeAtAnthropic, titleLines: 3),
        ]),
        if (linked && (usageState == ClaudeUsageState.limits || usageState == ClaudeUsageState.limitReached))
          Padding(
            key: const ValueKey('claude-usage'),
            padding: const EdgeInsets.fromLTRB(20, 16, 20, 0),
            child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [for (final l in _usage!.limits) ClaudeLimitBar(limit: l)]),
          ),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(32, 8, 32, 0),
            child: Text(_error!, style: TextStyle(fontSize: 13, color: Tokens.negative)),
          ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 20, 16, 0),
          child: linked
              ? Wrap(spacing: 8, runSpacing: 8, children: [
                  OutlinedButton(key: const ValueKey('claude-check'), onPressed: _busy ? null : () => _check(), child: const Text('Check now')),
                  OutlinedButton(key: const ValueKey('claude-reconnect'), onPressed: _busy ? null : _connect, child: const Text('Reconnect')),
                  OutlinedButton(
                    key: const ValueKey('claude-disconnect'),
                    style: OutlinedButton.styleFrom(foregroundColor: Tokens.negative),
                    onPressed: _busy ? null : _disconnect,
                    child: const Text('Disconnect'),
                  ),
                ])
              : FilledButton(
                  key: const ValueKey('claude-connect'),
                  style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(50)),
                  onPressed: _busy ? null : _connect,
                  child: Text(login?.status == ClaudeLoginStatus.expired ? 'Reconnect' : 'Connect your own Claude account (at Anthropic)'),
                ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 20, 20, 0),
          child: Text(claudeLoginOwnAccount, style: TextStyle(fontSize: 13, color: Tokens.label2)),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 8, 20, 24),
          child: Text(claudeLoginRisk, key: const ValueKey('claude-risk'), style: TextStyle(fontSize: 13, color: Tokens.label2)),
        ),
        ],
      ),
    );
  }
}

/// How long the page waits before it asks `/usage` again after an empty first answer.
Duration usageRetry = const Duration(seconds: 3);

/// One limit of the own login: a bar with its percentage and reset (ui-ontwerp §4).
class ClaudeLimitBar extends StatelessWidget {
  const ClaudeLimitBar({super.key, required this.limit});

  final PlanLimitView limit;

  @override
  Widget build(BuildContext context) {
    final full = limit.percent >= 100;
    final color = full ? Tokens.negative : (limit.percent >= 70 ? Tokens.warning : Tokens.accent);
    final text = [
      '${limit.percent.clamp(0, 100)} %',
      if (limit.resets != null) 'resets ${limit.resets}',
    ].join(' · ');
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Row(children: [
          Expanded(child: Text(limit.label, style: const TextStyle(fontSize: 15))),
          Text(text, style: TextStyle(fontSize: 13, color: Tokens.label2)),
        ]),
        const SizedBox(height: 6),
        ClipRRect(
          borderRadius: BorderRadius.circular(3),
          child: LinearProgressIndicator(
            value: (limit.percent / 100).clamp(0, 1).toDouble(),
            minHeight: 6,
            color: color,
            backgroundColor: Tokens.fill,
            semanticsLabel: limit.label,
          ),
        ),
      ]),
    );
  }
}

/// Connect (P-32): starts `claude_login.link` and walks through what Claude
/// asks. Answers true once the login is connected.
Future<bool> showConnectSheet(BuildContext context) async {
  final repository = context.read<AureliusRepository>();
  final bloc = context.read<ConnectionBloc>();
  return await showModalBottomSheet<bool>(
        context: context,
        isScrollControlled: true,
        isDismissible: false,
        builder: (_) => MultiBlocProvider(
          providers: [BlocProvider.value(value: bloc)],
          child: RepositoryProvider.value(value: repository, child: const ConnectSheet()),
        ),
      ) ??
      false;
}

enum _Step { starting, waiting, asking, checking, failed, command }

/// After the code, how often and how long the sheet asks whether Claude linked it.
Duration connectCheckEvery = const Duration(seconds: 2);
Duration connectCheckFor = const Duration(minutes: 2);

class ConnectSheet extends StatefulWidget {
  const ConnectSheet({super.key});

  @override
  State<ConnectSheet> createState() => _ConnectSheetState();
}

class _ConnectSheetState extends State<ConnectSheet> {
  final _code = TextEditingController();
  StreamSubscription<ClaudeLoginPrompt>? _prompts;
  StreamSubscription<LinkState>? _link;
  _Step _step = _Step.starting;
  String? _linkId;
  Uri? _url;
  String? _problem;
  bool _done = false;

  @override
  void initState() {
    super.initState();
    _code.addListener(() => setState(() {}));
    final repository = context.read<AureliusRepository>();
    // Listen first, so the prompt that follows the link is not missed.
    _prompts = repository.claudeLoginPrompts.listen(_onPrompt);
    _link = context.read<ConnectionBloc>().stream.listen(_onLink);
    _start();
  }

  @override
  void dispose() {
    _prompts?.cancel();
    _link?.cancel();
    _code.clear();
    _code.dispose();
    // Closed before it ended: stop Claude's process on the server (C13).
    final id = _linkId;
    if (!_done && id != null && _step != _Step.failed && _step != _Step.command) {
      unawaited(context.read<AureliusRepository>().cancelClaudeLogin(id).catchError((Object _) {}));
    }
    super.dispose();
  }

  Future<void> _start() async {
    setState(() {
      _step = _Step.starting;
      _problem = null;
      _url = null;
      _linkId = null;
      _code.clear();
    });
    try {
      final id = await context.read<AureliusRepository>().linkClaudeLogin();
      if (!mounted) return;
      setState(() {
        _linkId = id;
        if (_step == _Step.starting) _step = _Step.waiting;
      });
    } on ProtocolException catch (e) {
      if (!mounted) return;
      setState(() {
        // Without the guided flow the terminal route stays (P-32 fallback).
        _step = e.notOnServer ? _Step.command : _Step.failed;
        _problem = e.code == ErrorCode.tooManyAttempts ? 'Too many Connect attempts. Try again in a few minutes.' : e.shown;
      });
    } catch (e) {
      if (mounted) {
        setState(() {
          _step = _Step.failed;
          _problem = '$e';
        });
      }
    }
  }

  void _onPrompt(ClaudeLoginPrompt prompt) {
    if (!mounted || (_linkId != null && prompt.linkId != _linkId)) return;
    setState(() {
      _linkId ??= prompt.linkId;
      if (prompt.error != null) {
        _step = _Step.failed;
        _problem = prompt.error;
        _code.clear();
        return;
      }
      final url = prompt.url == null ? null : Uri.tryParse(prompt.url!);
      if (url != null && url.scheme == 'https') _url = url;
      if (prompt.asksCode) {
        _step = _Step.asking;
      } else if (_url == null) {
        // Claude showed nothing the phone can use: the terminal route (C12).
        _step = _Step.command;
      }
    });
  }

  void _onLink(LinkState link) {
    if (!mounted || _done) return;
    if (link.claudeLogin?.status == ClaudeLoginStatus.linked && (_step == _Step.checking || _step == _Step.asking)) _finish();
    // Signed out meanwhile: the server stopped the link (C13).
    if (link.user == null) Navigator.of(context).maybePop(false);
  }

  void _finish() {
    if (_done) return;
    _done = true;
    Navigator.of(context).pop(true);
  }

  Future<void> _awaitLinked(AureliusRepository repository) async {
    final until = DateTime.now().add(connectCheckFor);
    while (mounted && !_done && _step == _Step.checking && DateTime.now().isBefore(until)) {
      await Future<void>.delayed(connectCheckEvery);
      if (!mounted || _done || _step != _Step.checking) return;
      try {
        // Asked through the connection: the page and the Account row follow.
        final login = await repository.claudeLogin();
        if (login?.status == ClaudeLoginStatus.linked) return _finish();
      } on ProtocolException {
        // Asked again next round.
      } on DisconnectedException {
        // Idem.
      }
    }
    if (mounted && !_done && _step == _Step.checking) {
      setState(() {
        _step = _Step.failed;
        _problem = 'Claude did not confirm the code. Start again.';
      });
    }
  }

  Future<void> _paste() async {
    final text = (await Clipboard.getData(Clipboard.kTextPlain))?.text?.trim();
    if (!mounted || text == null || text.isEmpty) return;
    _code.text = text;
  }

  Future<void> _send() async {
    final id = _linkId;
    final code = _code.text.trim();
    if (id == null || !claudeCodeShape.hasMatch(code)) return;
    setState(() => _step = _Step.checking);
    _code.clear();
    try {
      final repository = context.read<AureliusRepository>();
      final login = await repository.sendClaudeLoginCode(id, code);
      if (!mounted) return;
      repository.applyClaudeLogin(login);
      if (login.status == ClaudeLoginStatus.linked) return _finish();
      // The server answers before Claude finished (QA3 M1): ask the status
      // until it is linked, besides `claude_login.updated`, so the sheet
      // closes even when that event is late or missed.
      await _awaitLinked(repository);
    } on ProtocolException catch (e) {
      // Never the code itself: a fixed text (C8, C14).
      if (mounted) {
        setState(() {
          _step = _Step.failed;
          // The daemon's own fixed text, word for word (F2); it never holds the code.
          _problem = e.notOnServer ? e.shown : (e.message.trim().isNotEmpty ? e.message : 'The code was not accepted. Start again.');
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _step = _Step.failed;
          _problem = 'The code was not accepted. Start again.';
        });
      }
    }
  }

  Future<void> _cancel() async {
    final id = _linkId;
    _done = true;
    if (id != null && _step != _Step.failed && _step != _Step.command) {
      try {
        await context.read<AureliusRepository>().cancelClaudeLogin(id);
      } catch (_) {
        // The server ends it after 10 minutes anyway.
      }
    }
    if (mounted) Navigator.of(context).pop(false);
  }

  Widget _number(String n) => Container(
        width: 24,
        height: 24,
        alignment: Alignment.center,
        decoration: BoxDecoration(shape: BoxShape.circle, color: Tokens.accentTint),
        child: Text(n, style: TextStyle(fontSize: 13, fontWeight: FontWeight.w600, color: Tokens.accent)),
      );

  Widget _command() {
    final userId = context.read<ConnectionBloc>().state.user?.id ?? '<your id>';
    final command = claudeLoginCommand(userId);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      const SizedBox(height: 12),
      Text('On the server, in a terminal:', style: TextStyle(fontSize: 13, color: Tokens.label2)),
      const SizedBox(height: 6),
      Container(
        padding: const EdgeInsets.all(10),
        decoration: BoxDecoration(color: Tokens.fill, borderRadius: BorderRadius.circular(8)),
        child: Row(children: [
          Expanded(child: SelectableText(command, key: const ValueKey('claude-command'), style: TextStyle(fontFamily: Tokens.mono, fontSize: 12))),
          CopyButton(text: command),
        ]),
      ),
      const SizedBox(height: 6),
      Text('Add --console or --sso for those sign-ins. Then come back and pull to refresh.', style: TextStyle(fontSize: 12, color: Tokens.label2)),
    ]);
  }

  @override
  Widget build(BuildContext context) {
    final url = _url;
    final asking = _step == _Step.asking;
    final busy = _step == _Step.starting || _step == _Step.waiting || _step == _Step.checking;
    return Padding(
      padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
      child: SafeArea(
        child: SingleChildScrollView(
          padding: const EdgeInsets.fromLTRB(20, 20, 20, 16),
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Text('Connect your Claude login', style: Tokens.title(size: 22)),
            const SizedBox(height: 4),
            Text('You sign in yourself, at Anthropic.', style: TextStyle(fontSize: 13, color: Tokens.label2)),
            const SizedBox(height: 16),
            if (_step == _Step.failed) ...[
              Text(_problem ?? 'Connecting did not work.', key: const ValueKey('connect-problem'), style: TextStyle(color: Tokens.negative)),
              const SizedBox(height: 12),
              FilledButton(key: const ValueKey('connect-again'), onPressed: _start, child: const Text('Try again')),
            ] else if (_step == _Step.command) ...[
              if (_problem != null) Text(_problem!, key: const ValueKey('connect-problem'), style: TextStyle(color: Tokens.label2)),
              _command(),
            ] else ...[
              Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                _number('1'),
                const SizedBox(width: 10),
                const Expanded(child: Text('Open the Claude sign-in page and sign in yourself.')),
              ]),
              Padding(
                padding: const EdgeInsets.only(left: 34, top: 8),
                child: Wrap(spacing: 8, runSpacing: 8, children: [
                  FilledButton.tonalIcon(
                    key: const ValueKey('connect-open'),
                    onPressed: url == null ? null : () => openClaudeSignIn(url),
                    icon: const Icon(CupertinoIcons.arrow_up_right_square, size: 18),
                    label: const Text('Open sign-in page'),
                  ),
                  if (url != null) CopyButton(text: url.toString(), label: true, tooltip: 'Copy link'),
                ]),
              ),
              const SizedBox(height: 16),
              Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                _number('2'),
                const SizedBox(width: 10),
                const Expanded(child: Text('Paste the code Claude shows you.')),
              ]),
              Padding(
                padding: const EdgeInsets.only(left: 34, top: 8),
                child: TextField(
                  key: const ValueKey('connect-code'),
                  controller: _code,
                  enabled: asking,
                  autocorrect: false,
                  enableSuggestions: false,
                  enableIMEPersonalizedLearning: false,
                  keyboardType: TextInputType.visiblePassword,
                  textInputAction: TextInputAction.done,
                  onSubmitted: (_) => _send(),
                  decoration: InputDecoration(
                    labelText: 'Code',
                    errorText: _code.text.isNotEmpty && !claudeCodeShape.hasMatch(_code.text.trim()) ? 'Only the code, as Claude shows it.' : null,
                    suffixIcon: TextButton(key: const ValueKey('connect-paste'), onPressed: asking ? _paste : null, child: const Text('Paste')),
                  ),
                ),
              ),
              if (busy)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: Row(children: [
                    const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2)),
                    const SizedBox(width: 10),
                    Text(_step == _Step.checking ? 'Checking the code with Claude…' : 'Asking Claude for the sign-in page…', style: TextStyle(fontSize: 13, color: Tokens.label2)),
                  ]),
                ),
            ],
            const SizedBox(height: 16),
            Text('Aurelius never sees your Claude password.', style: TextStyle(fontSize: 13, color: Tokens.label2)),
            const SizedBox(height: 6),
            Text(claudeLoginRisk, key: const ValueKey('connect-risk'), style: TextStyle(fontSize: 12, color: Tokens.label2)),
            if (_step != _Step.command)
              Align(
                alignment: Alignment.centerLeft,
                child: TextButton(key: const ValueKey('connect-show-command'), onPressed: () => setState(() => _step = _Step.command), child: const Text('Prefer a terminal? Show command')),
              ),
            const SizedBox(height: 8),
            Row(mainAxisAlignment: MainAxisAlignment.end, children: [
              TextButton(key: const ValueKey('connect-cancel'), onPressed: _cancel, child: Text(_step == _Step.command || _step == _Step.failed ? 'Close' : 'Cancel')),
              const SizedBox(width: 8),
              FilledButton(
                key: const ValueKey('connect-send'),
                onPressed: asking && claudeCodeShape.hasMatch(_code.text.trim()) ? _send : null,
                child: const Text('Connect'),
              ),
            ]),
          ]),
        ),
      ),
    );
  }
}

/// The reminder on Home (ui-ontwerp §4 "Herinnering zonder login", mobile).
class ClaudeLoginCard extends StatelessWidget {
  const ClaudeLoginCard({super.key});

  @override
  Widget build(BuildContext context) {
    final link = context.watch<ConnectionBloc>().state;
    if (!claudeLoginMissing(link)) return const SizedBox.shrink();
    final expired = link.claudeLogin?.status == ClaudeLoginStatus.expired;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
      child: AttentionCard(
        key: const ValueKey('claude-login-card'),
        onTap: () => context.push('/account/claude'),
        child: Row(children: [
          Expanded(
            child: Text(
              expired ? 'Your Claude login expired. Reconnect it to run your tickets.' : 'Connect your Claude login to run your tickets.',
              style: const TextStyle(fontSize: 15),
            ),
          ),
          const SizedBox(width: 8),
          TextButton(onPressed: () => context.push('/account/claude'), child: Text(expired ? 'Reconnect' : 'Connect')),
        ]),
      ),
    );
  }
}
