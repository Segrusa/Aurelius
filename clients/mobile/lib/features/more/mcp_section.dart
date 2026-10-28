import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import 'mcp_login_sheet.dart';

/// The user's Claude Code MCP servers with one on/off switch each. On means every
/// chat, conversation, employee and task can use the server. Each switch sends the
/// whole enabled list (`mcp.set`) and shows the list the server answers with.
/// Only the server's own summary is shown, never secrets. Remote servers show
/// their login status with Log in / Log out (servers that leave `auth` out, or
/// do not know the login methods, show none of it).
class McpSection extends StatefulWidget {
  const McpSection({super.key});

  @override
  State<McpSection> createState() => _McpSectionState();
}

class _McpSectionState extends State<McpSection> {
  late final AureliusRepository _repository = context.read<AureliusRepository>();
  McpServers? _servers;
  String? _error;
  bool _busy = false;
  String? _loginError;
  bool _loginUnsupported = false;
  StreamSubscription<Event>? _authChanges;

  bool get _canApprove => _repository.linkState.can('approve');

  @override
  void initState() {
    super.initState();
    _run(_repository.mcpServers);
    _authChanges = _repository.mcpAuthChanges.listen((_) => _refresh());
  }

  @override
  void dispose() {
    _authChanges?.cancel();
    super.dispose();
  }

  /// Reload without the spinner (after a login or logout, here or elsewhere).
  Future<void> _refresh() async {
    try {
      final result = await _repository.mcpServers();
      if (mounted) setState(() => _servers = result);
    } catch (_) {
      // The list on screen stays; Retry or the next change reloads.
    }
  }

  /// An older server answers the login methods with an unknown-method error.
  bool _unknownMethod(Object e) =>
      e is ProtocolException && e.code == ErrorCode.invalidRequest && (e.message.contains('mcp.log') || e.message.contains('unknown variant'));

  void _loginFailed(Object e) {
    if (!mounted) return;
    setState(() {
      if (_unknownMethod(e)) {
        _loginUnsupported = true;
        _loginError = null;
      } else if (e is ProtocolException) {
        _loginError = e.isForbidden ? mcpLoginNeedsApprove : (e.message.isEmpty ? 'The login could not be started.' : e.message);
      } else {
        _loginError = e.toString();
      }
    });
  }

  Future<void> _logIn(String name) async {
    setState(() {
      _busy = true;
      _loginError = null;
    });
    McpLogin? login;
    try {
      login = await _repository.startMcpLogin(name);
    } catch (e) {
      _loginFailed(e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
    if (login == null || !mounted) return;
    final ok = await showMcpLoginSheet(context, _repository, login);
    if (!mounted) return;
    if (ok) toast(context, 'Logged in to $name.');
    await _refresh();
  }

  Future<void> _logOut(String name) async {
    final sure = await confirm(context,
        title: 'Log out of $name?', body: 'Runs can no longer use this server until you log in again.', action: 'Log out', danger: true);
    if (!sure || !mounted) return;
    setState(() {
      _busy = true;
      _loginError = null;
    });
    try {
      final result = await _repository.logoutMcp(name);
      if (mounted) setState(() => _servers = result);
    } catch (e) {
      _loginFailed(e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _run(Future<McpServers> Function() call) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final result = await call();
      if (mounted) setState(() => _servers = result);
    } on ProtocolException catch (e) {
      if (mounted) setState(() => _error = e.isForbidden ? mcpNeedsApprove : (e.message.isEmpty ? 'MCP servers could not be loaded.' : e.message));
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  void _toggle(String name, bool on) {
    final enabled = _servers!.enabledNames.where((n) => n != name).toList();
    if (on) enabled.add(name);
    _run(() => _repository.setMcpServers(enabled));
  }

  void _remove(String name) {
    final enabled = _servers!.enabledNames.where((n) => n != name).toList();
    _run(() => _repository.setMcpServers(enabled));
  }

  @override
  Widget build(BuildContext context) {
    final muted = TextStyle(color: Tokens.label3, fontSize: 13);
    final servers = _servers;
    return Column(
      key: const ValueKey('mcp-section'),
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const GroupHeader(
          'MCP servers',
          caps: true,
          info: 'On means every chat, conversation, employee and task can use the server. Changes apply from the next run. '
              'Add servers with `claude mcp add --scope user`.',
        ),
        if (!_canApprove) Padding(padding: const EdgeInsets.fromLTRB(32, 0, 32, 8), child: Text(mcpNeedsApprove, style: muted)),
        if (_loginError != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(32, 0, 32, 8),
            child: Text(_loginError!, key: const ValueKey('mcp-login-start-error'), style: TextStyle(color: Tokens.red, fontSize: 13)),
          ),
        if (_error != null)
          InsetGroup(children: [
            GroupRow(
              key: const ValueKey('mcp-error'),
              title: _error!,
              titleLines: 3,
              trailing: TextButton(onPressed: _busy ? null : () => _run(_repository.mcpServers), child: const Text('Retry')),
            ),
          ])
        else if (servers == null)
          const InsetGroup(children: [GroupRow(title: 'Loading…')])
        else if (servers.servers.isEmpty && servers.missing.isEmpty)
          Padding(
            key: const ValueKey('mcp-empty'),
            padding: const EdgeInsets.fromLTRB(32, 4, 32, 8),
            child: Text('No MCP servers yet.', style: muted),
          )
        else
          InsetGroup(children: [
            for (final s in servers.servers) ...[
              GroupRow(
                key: ValueKey('mcp-${s.name}'),
                title: s.name,
                subtitle: s.summary.isNotEmpty ? s.summary : s.transport.wire,
                trailing: CupertinoSwitch(
                  value: s.enabled,
                  activeTrackColor: Tokens.accent,
                  onChanged: _canApprove && !_busy ? (v) => _toggle(s.name, v) : null,
                ),
              ),
              if (!_loginUnsupported && s.authLabel != null) _authRow(s),
            ],
            for (final name in servers.missing)
              GroupRow(
                key: ValueKey('mcp-missing-$name'),
                title: name,
                subtitle: 'Enabled, but Claude Code no longer has this server.',
                trailing: TextButton(onPressed: _canApprove && !_busy ? () => _remove(name) : null, child: const Text('Remove')),
              ),
          ]),
      ],
    );
  }

  /// The login line under a remote server: status badge and Log in / Log out.
  Widget _authRow(McpServerInfo s) {
    final color = switch (s.auth) {
      McpAuthStatus.authorized => Tokens.accent,
      McpAuthStatus.needsLogin => Tokens.orange,
      _ => Tokens.gray,
    };
    final enabled = _canApprove && !_busy;
    return GroupRow(
      key: ValueKey('mcp-auth-${s.name}'),
      padding: const EdgeInsets.fromLTRB(32, 2, 16, 2),
      leading: Icon(Icons.circle, size: 10, color: color),
      title: s.authLabel!,
      titleStyle: TextStyle(color: color, fontSize: 13.5, fontWeight: FontWeight.w600),
      trailing: s.canLogOut
          ? TextButton(key: ValueKey('mcp-logout-${s.name}'), onPressed: enabled ? () => _logOut(s.name) : null, child: const Text('Log out'))
          : TextButton(key: ValueKey('mcp-login-${s.name}'), onPressed: enabled ? () => _logIn(s.name) : null, child: const Text('Log in')),
    );
  }
}

/// The MCP tools answered with 'Always allow': they run without asking in every
/// project (Plan mode still asks). Read-only: tools are taken off the list on
/// the desktop. Hidden until the list loads, so servers without it show nothing.
class McpAlwaysAllowSection extends StatefulWidget {
  const McpAlwaysAllowSection({super.key});

  @override
  State<McpAlwaysAllowSection> createState() => _McpAlwaysAllowSectionState();
}

class _McpAlwaysAllowSectionState extends State<McpAlwaysAllowSection> {
  late final AureliusRepository _repository = context.read<AureliusRepository>();
  late final StreamSubscription<Event> _changes;
  List<String>? _tools;

  @override
  void initState() {
    super.initState();
    _load();
    // A decision with 'Always allow' (here or elsewhere) adds to the list.
    _changes = _repository.companyChanges
        .where((e) => e.kind == EventKind.approvalConsumed || e.kind == EventKind.approvalAllowedAlways)
        .listen((_) => _load());
  }

  @override
  void dispose() {
    _changes.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final tools = await _repository.mcpAlwaysAllowed();
      if (mounted) setState(() => _tools = tools);
    } catch (_) {
      // Older servers do not know the list: the section stays hidden.
    }
  }

  @override
  Widget build(BuildContext context) {
    final tools = _tools;
    if (tools == null) return const SizedBox.shrink();
    final muted = TextStyle(color: Tokens.label3, fontSize: 13);
    return Column(
      key: const ValueKey('mcp-always-allow-section'),
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const GroupHeader(
          'Always allowed tools',
          caps: true,
          info: 'These MCP tools run without asking, in every project. Plan mode still asks. Take one off the list on the desktop. '
              'Choose Always allow on an approval of an MCP tool to add it here.',
        ),
        if (tools.isEmpty)
          Padding(
            key: const ValueKey('mcp-always-allow-empty'),
            padding: const EdgeInsets.fromLTRB(32, 4, 32, 8),
            child: Text('No tools yet.', style: muted),
          )
        else
          InsetGroup(children: [
            for (final tool in tools)
              GroupRow(
                key: ValueKey('mcp-always-$tool'),
                title: mcpToolName(tool),
                subtitle: tool,
              ),
          ]),
      ],
    );
  }
}
