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
import '../company/company_bloc.dart';
import 'add_plugin_sheet.dart';
import 'plugin_widgets.dart';

/// Settings → Plugins: the Claude Code plugins installed on the PC, with the
/// employees that use each, Update, Remove and Add plugin. Reading needs the
/// Read right, changing needs Approve.
class PluginsPage extends StatefulWidget {
  const PluginsPage({super.key});

  @override
  State<PluginsPage> createState() => _PluginsPageState();
}

class _PluginsPageState extends State<PluginsPage> {
  late final AureliusRepository _repository = context.read<AureliusRepository>();
  List<PluginInfo>? _plugins;
  String? _error;
  String? _busyId;

  bool get _canApprove => _repository.linkState.can('approve');

  @override
  void initState() {
    super.initState();
    _reload();
  }

  Future<void> _reload() => _run(null, _repository.pluginsList);

  /// Runs [call], shows the list it answers with, or a readable error.
  Future<void> _run(String? id, Future<PluginsSnapshot> Function() call, {String? done}) async {
    setState(() {
      _busyId = id;
      _error = null;
    });
    try {
      final snapshot = await call();
      if (!mounted) return;
      setState(() => _plugins = snapshot.plugins);
      if (done != null) toast(context, done);
    } on ProtocolException catch (e) {
      if (mounted) setState(() => _error = e.isForbidden ? pluginsNeedsApprove : pluginError(e, 'The plugin call failed.'));
    } catch (e) {
      if (mounted) setState(() => _error = pluginError(e, 'The plugin call failed.'));
    } finally {
      if (mounted) setState(() => _busyId = null);
    }
  }

  Future<void> _remove(PluginInfo p) async {
    final ok = await confirm(context,
        title: 'Remove ${p.name}?', body: 'It is uninstalled from this PC and taken off every employee.', action: 'Remove', danger: true);
    if (ok && mounted) _run(p.id, () => _repository.pluginRemoveOne(p.id), done: '${p.name} removed');
  }

  Future<void> _add() async {
    final added = await showAddPluginSheet(context);
    if (added == true && mounted) _reload();
  }

  @override
  Widget build(BuildContext context) {
    final employees = context.watch<CompanyBloc>().state.employees;
    final projects = context.watch<CompanyBloc>().state.projects;
    String nameOf(String id) => employees.where((m) => m.id == id).firstOrNull?.name ?? id;
    String projectOf(String id) => 'Project ${projects.where((p) => p.id == id).firstOrNull?.name ?? id}';
    final plugins = _plugins;
    final muted = TextStyle(color: Tokens.label3, fontSize: 13);
    return Scaffold(
      appBar: GlassAppBar(
        title: const Text('Plugins'),
        actions: [
          const InfoIcon(
            'Claude Code plugins on this PC. An employee uses a plugin from its next run. Assigning a plugin that runs hooks lets it run commands on this PC.',
            label: 'Plugins',
          ),
          TextButton(key: const ValueKey('plugins-add'), onPressed: _canApprove && _busyId == null ? _add : null, child: const Text('Add plugin')),
        ],
      ),
      body: RefreshIndicator.adaptive(
              onRefresh: _reload,
              child: ListView(
                physics: const AlwaysScrollableScrollPhysics(parent: BouncingScrollPhysics()),
                padding: EdgeInsets.fromLTRB(0, 12, 0, 32 + MediaQuery.paddingOf(context).bottom),
                children: [
                  if (!_canApprove) Padding(padding: const EdgeInsets.fromLTRB(32, 0, 32, 8), child: Text(pluginsNeedsApprove, style: muted)),
                  if (_error != null)
                    InsetGroup(children: [
                      GroupRow(
                        key: const ValueKey('plugins-error'),
                        title: _error!,
                        titleLines: 6,
                        trailing: TextButton(onPressed: _busyId == null ? _reload : null, child: const Text('Retry')),
                      ),
                    ]),
                  if (plugins == null && _error == null)
                    const Padding(padding: EdgeInsets.all(32), child: Center(child: CupertinoActivityIndicator()))
                  else if (plugins != null && plugins.isEmpty)
                    Padding(
                      key: const ValueKey('plugins-empty'),
                      padding: const EdgeInsets.fromLTRB(32, 8, 32, 8),
                      child: Text('No plugins yet. Tap Add plugin and paste a link.', style: muted),
                    )
                  else if (plugins != null)
                    for (final p in plugins)
                      Padding(
                        padding: const EdgeInsets.only(bottom: 12),
                        child: InsetGroup(children: [
                          _PluginCard(
                            p,
                            nameOf: nameOf,
                            projectOf: projectOf,
                            busy: _busyId == p.id,
                            canApprove: _canApprove && _busyId == null,
                            onUpdate: () => _run(p.id, () => _repository.pluginUpdateOne(p.id), done: '${p.name} updated'),
                            onRemove: () => _remove(p),
                          ),
                        ]),
                      ),
                ],
              ),
            ),
    );
  }
}

class _PluginCard extends StatelessWidget {
  const _PluginCard(this.plugin, {required this.nameOf, required this.projectOf, required this.busy, required this.canApprove, required this.onUpdate, required this.onRemove});

  final PluginInfo plugin;
  final String Function(String) nameOf;
  final String Function(String) projectOf;
  final bool busy;
  final bool canApprove;
  final VoidCallback onUpdate;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final p = plugin;
    final used = [...p.employees.map(nameOf), ...p.projects.map(projectOf)];
    return Padding(
      key: ValueKey('plugin-${p.id}'),
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
            Flexible(child: Text(p.name, style: TextStyle(color: Tokens.label, fontSize: 17, fontWeight: FontWeight.w600))),
            if (p.version.isNotEmpty) ...[const SizedBox(width: 8), Text(p.version, style: TextStyle(color: Tokens.muted, fontSize: 13))],
          ]),
          if (p.marketplace.isNotEmpty) Text(p.marketplace, style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
          if (p.description.isNotEmpty) Padding(padding: const EdgeInsets.only(top: 6), child: Text(p.description, style: TextStyle(color: Tokens.text2, fontSize: 14))),
          if (p.missing)
            Padding(padding: const EdgeInsets.only(top: 6), child: Text('Its folder is missing on this PC.', style: TextStyle(color: Tokens.negative, fontSize: 13))),
          const SizedBox(height: 10),
          PluginChips(p.components),
          PluginNotes(p.components),
          Padding(
            padding: const EdgeInsets.only(top: 10),
            child: Text(
              used.isEmpty ? 'Used by nobody yet' : 'Used by ${used.join(', ')}',
              key: ValueKey('plugin-used-${p.id}'),
              style: TextStyle(color: Tokens.text2, fontSize: 13),
            ),
          ),
          const SizedBox(height: 4),
          if (busy)
            const Padding(padding: EdgeInsets.only(top: 8), child: LinearProgressIndicator())
          else
            Row(children: [
              TextButton(key: ValueKey('plugin-update-${p.id}'), onPressed: canApprove ? onUpdate : null, child: const Text('Update')),
              TextButton(
                key: ValueKey('plugin-remove-${p.id}'),
                onPressed: canApprove ? onRemove : null,
                child: Text('Remove', style: TextStyle(color: canApprove ? Tokens.negative : Tokens.label3)),
              ),
            ]),
        ],
      ),
    );
  }
}
