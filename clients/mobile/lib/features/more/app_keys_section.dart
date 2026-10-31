import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/protocol/models.dart';
import '../account/claude_login.dart' show apiKeyLoginNote, ownSubscriptionNote;

const appKeysNeedApprove = 'Managing app keys needs the Approve right.';

/// The webhook address of an app key: `http://` or `https://` with a host.
/// Empty is fine (no webhook). Answers the problem, or null when it is valid.
String? webhookUrlProblem(String value) {
  final text = value.trim();
  if (text.isEmpty) return null;
  final uri = Uri.tryParse(text);
  if (uri == null || !(uri.scheme == 'http' || uri.scheme == 'https') || uri.host.isEmpty) {
    return 'Use an http:// or https:// address.';
  }
  return null;
}

/// Keys of outside apps (feature `app_keys`): the list with audience and
/// webhook address (never a secret), Create and Revoke. A new key shows its
/// secret, and the webhook secret when it has a webhook, once.
class AppKeysSection extends StatefulWidget {
  const AppKeysSection({super.key});

  @override
  State<AppKeysSection> createState() => _AppKeysSectionState();
}

class _AppKeysSectionState extends State<AppKeysSection> {
  late final AureliusRepository _repository = context.read<AureliusRepository>();
  List<AppKeyView>? _keys;
  String? _error;
  bool _busy = false;

  bool get _canApprove => _repository.linkState.can('approve');
  bool get _management => _repository.linkState.has('management_channel');
  bool get _webhooks => _repository.linkState.has('webhooks');

  @override
  void initState() {
    super.initState();
    if (_repository.linkState.has('app_keys')) _load();
  }

  Future<void> _load() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final keys = await _repository.appKeys();
      if (mounted) setState(() => _keys = keys);
    } on ProtocolException catch (e) {
      if (mounted) setState(() => _error = e.isForbidden ? appKeysNeedApprove : (e.message.isEmpty ? 'App keys could not be loaded.' : e.message));
    } catch (e) {
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _create() async {
    final created = await showModalBottomSheet<AppKeyCreated>(
      context: context,
      isScrollControlled: true,
      builder: (_) => CreateAppKeySheet(repository: _repository, management: _management, webhooks: _webhooks),
    );
    if (created == null || !mounted) return;
    await _load();
    if (!mounted) return;
    await showDialog<void>(context: context, barrierDismissible: false, builder: (_) => AppKeySecretsDialog(created: created));
  }

  Future<void> _revoke(AppKeyView key) async {
    final sure = await confirm(context,
        title: 'Revoke ${key.name}?', body: 'The app can no longer connect, and its open sessions end at once.', action: 'Revoke', danger: true);
    if (!sure || !mounted) return;
    setState(() => _busy = true);
    try {
      await _repository.revokeAppKey(key.id);
    } catch (e) {
      if (mounted) setState(() => _error = e is ProtocolException && e.message.isNotEmpty ? e.message : e.toString());
    }
    if (mounted) await _load();
  }

  String _subtitle(AppKeyView k) {
    final parts = <String>[
      if (_management || k.audience == AppAudience.management) 'Audience: ${k.audience == AppAudience.management ? 'management' : 'all'}',
      'Scopes: ${k.scopes.map((s) => s.wire).join(', ')}',
      if (k.webhookUrl != null && k.webhookUrl!.isNotEmpty) 'Webhook: ${k.webhookUrl}',
      if (k.revokedAtMs != null) 'Revoked',
    ];
    return parts.join('\n');
  }

  @override
  Widget build(BuildContext context) {
    if (!_repository.linkState.has('app_keys')) return const SizedBox.shrink();
    final muted = TextStyle(color: Tokens.label3, fontSize: 13);
    final keys = _keys;
    return Column(
      key: const ValueKey('app-keys-section'),
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const GroupHeader('App keys', caps: true, info: 'Outside apps connect to Aurelius with a key. The secret is shown once, when you make the key.'),
        if (!_canApprove) Padding(padding: const EdgeInsets.fromLTRB(32, 0, 32, 8), child: Text(appKeysNeedApprove, style: muted)),
        if (_error != null)
          InsetGroup(children: [
            GroupRow(
              key: const ValueKey('app-keys-error'),
              title: _error!,
              titleLines: 3,
              trailing: TextButton(onPressed: _busy ? null : _load, child: const Text('Retry')),
            ),
          ])
        else if (keys == null)
          const InsetGroup(children: [GroupRow(title: 'Loading…')])
        else
          InsetGroup(children: [
            for (final k in keys)
              GroupRow(
                key: ValueKey('app-key-${k.id}'),
                title: k.name,
                subtitle: _subtitle(k),
                titleLines: 2,
                trailing: k.revokedAtMs != null
                    ? null
                    : TextButton(
                        key: ValueKey('app-key-revoke-${k.id}'),
                        onPressed: _canApprove && !_busy ? () => _revoke(k) : null,
                        child: Text('Revoke', style: TextStyle(color: Tokens.red)),
                      ),
              ),
            GroupRow(
              key: const ValueKey('app-key-create'),
              title: 'New app key',
              titleStyle: TextStyle(color: Tokens.accent),
              onTap: _canApprove && !_busy ? _create : null,
            ),
          ]),
      ],
    );
  }
}

/// The form for a new key. Pops with the created key. The audience and webhook
/// fields only show when the server has the features `management_channel` and
/// `webhooks`. A management key cannot hold the `approve` scope.
class CreateAppKeySheet extends StatefulWidget {
  const CreateAppKeySheet({super.key, required this.repository, required this.management, required this.webhooks});

  final AureliusRepository repository;
  final bool management;
  final bool webhooks;

  @override
  State<CreateAppKeySheet> createState() => _CreateAppKeySheetState();
}

class _CreateAppKeySheetState extends State<CreateAppKeySheet> {
  final _name = TextEditingController();
  final _url = TextEditingController();
  final Set<AppScope> _scopes = {AppScope.read, AppScope.draft};
  AppAudience _audience = AppAudience.all;
  String? _problem;
  bool _busy = false;

  @override
  void dispose() {
    _name.dispose();
    _url.dispose();
    super.dispose();
  }

  void _setAudience(AppAudience audience) {
    setState(() {
      _audience = audience;
      if (audience == AppAudience.management) _scopes.remove(AppScope.approve);
    });
  }

  Future<void> _submit() async {
    final name = _name.text.trim();
    final url = _url.text.trim();
    if (name.isEmpty) return setState(() => _problem = 'Give the key a name.');
    if (_scopes.isEmpty) return setState(() => _problem = 'Choose at least one scope.');
    if (widget.webhooks) {
      final bad = webhookUrlProblem(url);
      if (bad != null) return setState(() => _problem = bad);
    }
    setState(() {
      _busy = true;
      _problem = null;
    });
    try {
      final created = await widget.repository.createAppKey(CreateAppKey(
        name: name,
        scopes: AppScope.values.where(_scopes.contains).toList(),
        audience: widget.management ? _audience : null,
        webhookUrl: widget.webhooks && url.isNotEmpty ? url : null,
      ));
      if (mounted) Navigator.pop(context, created);
    } on ProtocolException catch (e) {
      if (mounted) {
        setState(() {
          _busy = false;
          _problem = e.isForbidden ? appKeysNeedApprove : (e.message.isEmpty ? 'The key could not be made.' : e.message);
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _busy = false;
          _problem = e.toString();
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final muted = TextStyle(color: Tokens.label3, fontSize: 13);
    final management = _audience == AppAudience.management;
    return Padding(
      padding: EdgeInsets.fromLTRB(20, 16, 20, 16 + MediaQuery.of(context).viewInsets.bottom),
      child: SingleChildScrollView(
        child: Column(
          key: const ValueKey('app-key-form'),
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('New app key', style: TextStyle(fontSize: 17, fontWeight: FontWeight.w600)),
            const SizedBox(height: 4),
            Text(ownSubscriptionNote, key: const ValueKey('app-key-subscription'), style: muted),
            Text(apiKeyLoginNote, key: const ValueKey('app-key-login'), style: muted),
            const SizedBox(height: 12),
            TextField(key: const ValueKey('app-key-name'), controller: _name, decoration: const InputDecoration(labelText: 'Name')),
            const SizedBox(height: 12),
            Wrap(spacing: 8, children: [
              for (final scope in AppScope.values)
                FilterChip(
                  key: ValueKey('app-key-scope-${scope.wire}'),
                  label: Text(scope.wire),
                  selected: _scopes.contains(scope),
                  onSelected: scope == AppScope.approve && management
                      ? null
                      : (on) => setState(() => on ? _scopes.add(scope) : _scopes.remove(scope)),
                ),
            ]),
            if (management) Padding(padding: const EdgeInsets.only(top: 4), child: Text('A management key cannot have the approve scope.', style: muted)),
            if (widget.management) ...[
              const SizedBox(height: 12),
              Row(children: [
                Text('Audience', style: muted),
                InfoIcon(
                  management
                      ? 'Management: the app reaches only the CEO and the CEO\'s direct reports (CTO, CMO level).'
                      : 'All: the app reaches everything its scopes allow.',
                  label: 'Audience',
                ),
              ]),
              const SizedBox(height: 4),
              SegmentedButton<AppAudience>(
                key: const ValueKey('app-key-audience'),
                segments: const [
                  ButtonSegment(value: AppAudience.all, label: Text('All')),
                  ButtonSegment(value: AppAudience.management, label: Text('Management')),
                ],
                selected: {_audience},
                onSelectionChanged: (s) => _setAudience(s.first),
              ),
            ],
            if (widget.webhooks) ...[
              const SizedBox(height: 12),
              TextField(
                key: const ValueKey('app-key-webhook'),
                controller: _url,
                keyboardType: TextInputType.url,
                autocorrect: false,
                decoration: const InputDecoration(
                  labelText: 'Webhook address (optional)',
                  hintText: 'https://…',
                  suffixIcon: InfoIcon('Events are posted here, signed with a secret shown once.', label: 'Webhook address'),
                ),
              ),
            ],
            if (_problem != null)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Text(_problem!, key: const ValueKey('app-key-problem'), style: TextStyle(color: Tokens.red, fontSize: 13)),
              ),
            const SizedBox(height: 12),
            Row(mainAxisAlignment: MainAxisAlignment.end, children: [
              TextButton(onPressed: _busy ? null : () => Navigator.pop(context), child: const Text('Cancel')),
              TextButton(key: const ValueKey('app-key-submit'), onPressed: _busy ? null : _submit, child: const Text('Create')),
            ]),
          ],
        ),
      ),
    );
  }
}

/// The secrets of a new key, shown once with a copy button each.
class AppKeySecretsDialog extends StatelessWidget {
  const AppKeySecretsDialog({super.key, required this.created});

  final AppKeyCreated created;

  Widget _secret(String keyName, String label, String value) => Padding(
        padding: const EdgeInsets.only(top: 8),
        child: Row(children: [
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(label, style: TextStyle(color: Tokens.label3, fontSize: 12)),
              SelectableText(value, key: ValueKey(keyName), style: const TextStyle(fontFamily: 'monospace', fontSize: 13)),
            ]),
          ),
          CopyButton(text: value),
        ]),
      );

  @override
  Widget build(BuildContext context) {
    final webhookSecret = created.webhookSecret;
    return AlertDialog(
      key: const ValueKey('app-key-secrets'),
      title: Text('Key for ${created.key.name}'),
      content: SingleChildScrollView(
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          const Text('Copy these now. They are not shown again.'),
          _secret('app-key-secret', 'App key secret', created.secret),
          if (webhookSecret != null && webhookSecret.isNotEmpty) _secret('app-key-webhook-secret', 'Webhook secret', webhookSecret),
        ]),
      ),
      actions: [TextButton(onPressed: () => Navigator.pop(context), child: const Text('Done'))],
    );
  }
}
