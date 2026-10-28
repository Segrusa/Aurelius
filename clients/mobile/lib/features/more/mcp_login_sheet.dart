import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

/// Opens the login page in the phone's browser; tests replace it.
Future<bool> Function(Uri uri) openMcpLoginPage = (uri) => launchUrl(uri, mode: LaunchMode.externalApplication);

/// Walks the user through an MCP login started with `mcp.login_start`: the
/// phone's browser logs in and then lands on the server's localhost callback,
/// which does not load on the phone. The user pastes that address here and
/// the server finishes the login. Answers `true` when the login succeeded.
Future<bool> showMcpLoginSheet(BuildContext context, AureliusRepository repository, McpLogin login) async =>
    await showModalBottomSheet<bool>(
      context: context,
      isScrollControlled: true,
      builder: (_) => McpLoginSheet(repository: repository, login: login),
    ) ??
    false;

class McpLoginSheet extends StatefulWidget {
  const McpLoginSheet({super.key, required this.repository, required this.login});

  final AureliusRepository repository;
  final McpLogin login;

  @override
  State<McpLoginSheet> createState() => _McpLoginSheetState();
}

class _McpLoginSheetState extends State<McpLoginSheet> with WidgetsBindingObserver {
  late McpLogin _login = widget.login;
  final _address = TextEditingController();
  StreamSubscription<Event>? _finished;
  String? _error;
  String? _notice;
  bool _busy = false;
  bool _closed = false;

  bool get _pending => _login.state == McpLoginState.pending;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    // Also ends when the login finishes elsewhere (the browser on the PC, or it expired).
    _finished = widget.repository.mcpAuthChanges
        .where((e) => e.kind == EventKind.mcpLoginFinished && e.aggregateId == _login.loginId)
        .listen((_) => _check());
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _open();
      _pasteFromClipboard();
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _finished?.cancel();
    _address.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    // Back from the browser: the address may be on the clipboard by now.
    if (state == AppLifecycleState.resumed) _pasteFromClipboard();
  }

  Future<void> _pasteFromClipboard({bool always = false}) async {
    if (!always && _address.text.isNotEmpty) return;
    final text = (await Clipboard.getData(Clipboard.kTextPlain))?.text?.trim();
    if (!mounted || text == null || text.isEmpty) return;
    if (always || looksLikeMcpCallback(text, redirectUri: _login.redirectUri)) {
      setState(() => _address.text = text);
    }
  }

  Future<void> _open() async {
    final uri = Uri.tryParse(_login.authorizationUrl);
    var opened = false;
    if (uri != null) {
      try {
        opened = await openMcpLoginPage(uri);
      } catch (_) {
        opened = false;
      }
    }
    if (opened || !mounted) return;
    await Clipboard.setData(ClipboardData(text: _login.authorizationUrl));
    if (mounted) setState(() => _notice = 'The browser could not be opened. The login link is copied: open it in your browser.');
  }

  Future<void> _check() async {
    try {
      _apply(await widget.repository.mcpLogin(_login.loginId));
    } catch (_) {
      // The paste field still works; the next event or Finish tries again.
    }
  }

  void _apply(McpLogin login) {
    if (!mounted || _closed) return;
    if (login.state == McpLoginState.succeeded) {
      _closed = true;
      Navigator.pop(context, true);
      return;
    }
    setState(() {
      _login = login;
      _error = mcpLoginProblem(login) ?? _error;
    });
  }

  Future<void> _finish() async {
    final address = _address.text.trim();
    if (address.isEmpty) {
      setState(() => _error = 'Paste the address of the page that did not load first.');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      _apply(await widget.repository.completeMcpLogin(_login.loginId, address));
    } on ProtocolException catch (e) {
      // A wrong address is refused and the login keeps waiting.
      if (mounted) setState(() => _error = e.message.isEmpty ? 'The login could not be finished.' : e.message);
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final muted = TextStyle(color: Tokens.muted, fontSize: 13.5);
    Widget step(String n, String text) => Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            SizedBox(width: 22, child: Text(n, style: TextStyle(color: Tokens.accentText, fontWeight: FontWeight.w600))),
            Expanded(child: Text(text)),
          ]),
        );
    return SafeArea(
      child: Padding(
        padding: EdgeInsets.fromLTRB(20, 4, 20, 16 + MediaQuery.viewInsetsOf(context).bottom),
        child: SingleChildScrollView(
          child: Column(
            key: const ValueKey('mcp-login-sheet'),
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('Log in to ${_login.name}', style: Tokens.title(size: 20)),
              const SizedBox(height: 12),
              step('1', 'Your browser opens the login page. Log in there and allow access.'),
              step('2', 'The browser then goes to a page that does not load (an address starting with http://localhost). That is expected.'),
              step('3', 'Copy the address of that page from the address bar and paste it below.'),
              const SizedBox(height: 4),
              TextField(
                key: const ValueKey('mcp-login-address'),
                controller: _address,
                enabled: _pending && !_busy,
                keyboardType: TextInputType.url,
                autocorrect: false,
                maxLines: 3,
                minLines: 1,
                decoration: InputDecoration(
                  labelText: 'Address of the page that did not load',
                  hintText: 'http://localhost:…/callback?code=…',
                  border: const OutlineInputBorder(),
                  suffixIcon: IconButton(
                    tooltip: 'Paste',
                    icon: const Icon(Icons.content_paste),
                    onPressed: _pending && !_busy ? () => _pasteFromClipboard(always: true) : null,
                  ),
                ),
                onSubmitted: (_) => _finish(),
              ),
              if (_notice != null) Padding(padding: const EdgeInsets.only(top: 10), child: Text(_notice!, style: muted)),
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 10),
                  child: Text(_error!, key: const ValueKey('mcp-login-error'), style: TextStyle(color: Tokens.red, fontSize: 13.5)),
                ),
              const SizedBox(height: 16),
              Row(children: [
                TextButton(onPressed: _pending && !_busy ? _open : null, child: const Text('Open again')),
                const Spacer(),
                TextButton(onPressed: () => Navigator.pop(context, false), child: Text(_pending ? 'Cancel' : 'Close')),
                const SizedBox(width: 8),
                FilledButton(
                  key: const ValueKey('mcp-login-finish'),
                  onPressed: _pending && !_busy ? _finish : null,
                  child: Text(_busy ? 'Finishing…' : 'Finish login'),
                ),
              ]),
            ],
          ),
        ),
      ),
    );
  }
}
