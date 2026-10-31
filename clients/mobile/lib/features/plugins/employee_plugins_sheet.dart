import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import 'add_plugin_sheet.dart';
import 'plugin_widgets.dart';

/// Plugins for one employee: a switch per installed plugin (`plugin.set_for_employee`),
/// enabled plugins that are no longer installed flagged, and Add plugin… with
/// this employee pre-ticked. In a task it also loads the plugins of the
/// task's project (Project settings).
/// Applies from the employee's next run; completes when the sheet closes.
Future<void> showEmployeePluginsSheet(BuildContext context, EmployeeSummary employee) {
  final repository = context.read<AureliusRepository>();
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (sheet) => SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.85),
        child: ListView(
          shrinkWrap: true,
          padding: const EdgeInsets.only(bottom: 12),
          children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Text('Plugins for ${employee.name}', style: Tokens.title(size: 20))),
            Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 8), child: Text('Applies from its next run.', style: TextStyle(color: Tokens.muted, fontSize: 12.5))),
            employeePluginSwitches(context, repository, employee.id),
          ],
        ),
      ),
    ),
  );
}

/// The switches for one employee, for a sheet or a page.
PluginSwitches employeePluginSwitches(BuildContext context, AureliusRepository repository, String employeeId) => PluginSwitches(
      key: ValueKey('employee-plugins-$employeeId'),
      repository: repository,
      get: () async => (await repository.employeePlugins(employeeId)).mine,
      set: (enabled) async => (await repository.setEmployeePlugins(employeeId, enabled)).mine,
      openAdd: () => showAddPluginSheet(context, employeeId: employeeId),
    );

typedef PluginsMine = ({List<String> enabled, List<String> missing, List<String> inherited});

extension on EmployeePlugins {
  PluginsMine get mine => (enabled: enabled, missing: missing, inherited: const []);
}

/// A switch per installed plugin for one list, with the hooks warning, missing
/// ones flagged and Add plugin…. Ids in `inherited` come from elsewhere (the
/// project) and cannot be switched off here.
class PluginSwitches extends StatefulWidget {
  const PluginSwitches({super.key, required this.repository, required this.get, required this.set, required this.openAdd, this.onChanged});

  final AureliusRepository repository;
  final Future<PluginsMine> Function() get;
  final Future<PluginsMine> Function(List<String> enabled) set;
  final Future<bool?> Function() openAdd;

  /// After the list changed on the server.
  final VoidCallback? onChanged;

  @override
  State<PluginSwitches> createState() => _PluginSwitchesState();
}

class _PluginSwitchesState extends State<PluginSwitches> {
  List<PluginInfo>? _installed;
  PluginsMine? _mine;
  String? _error;
  bool _busy = false;

  bool get _canApprove => widget.repository.linkState.can('approve');

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _guard(Future<void> Function() body) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await body();
    } on ProtocolException catch (e) {
      if (mounted) setState(() => _error = e.isForbidden ? pluginsNeedsApprove : pluginError(e, 'Plugins could not be loaded.'));
    } catch (e) {
      if (mounted) setState(() => _error = pluginError(e, 'Plugins could not be loaded.'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _load() => _guard(() async {
        final installed = await widget.repository.pluginsList();
        final mine = await widget.get();
        if (mounted) {
          setState(() {
            _installed = installed.plugins;
            _mine = mine;
          });
        }
      });

  Future<void> _set(List<String> enabled) => _guard(() async {
        final mine = await widget.set(enabled);
        if (mounted) setState(() => _mine = mine);
        widget.onChanged?.call();
      });

  void _toggle(String id, bool on) {
    final enabled = _mine!.enabled.where((e) => e != id).toList();
    if (on) enabled.add(id);
    _set(enabled);
  }

  Future<void> _add() async {
    final added = await widget.openAdd();
    if (added == true && mounted) {
      await _load();
      widget.onChanged?.call();
    }
  }

  @override
  Widget build(BuildContext context) {
    final muted = TextStyle(color: Tokens.muted, fontSize: 12.5);
    final installed = _installed;
    final mine = _mine;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (!_canApprove) Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 8), child: Text(pluginsNeedsApprove, style: muted)),
        if (_error != null)
          ListTile(
            key: const ValueKey('employee-plugins-error'),
            title: Text(_error!, style: TextStyle(color: Tokens.negative)),
            trailing: TextButton(onPressed: _busy ? null : _load, child: const Text('Retry')),
          ),
        if (installed == null && _error == null) const Padding(padding: EdgeInsets.all(24), child: Center(child: CircularProgressIndicator.adaptive())),
        if (installed != null && mine != null) ...[
          if (installed.isEmpty) ListTile(title: Text('No plugins installed yet.', style: TextStyle(color: Tokens.muted))),
          for (final p in installed)
            _row(p, mine, muted),
          for (final id in mine.missing)
            ListTile(
              key: ValueKey('employee-plugin-missing-$id'),
              title: Text(id),
              subtitle: Text('Enabled, but no longer installed.', style: TextStyle(color: Tokens.warning, fontSize: 12.5)),
              trailing: TextButton(onPressed: _canApprove && !_busy ? () => _toggle(id, false) : null, child: const Text('Remove')),
            ),
        ],
        if (_busy && installed != null) const LinearProgressIndicator(),
        ListTile(
          key: const ValueKey('employee-plugins-add'),
          leading: Icon(Icons.add, color: Tokens.accent),
          title: const Text('Add plugin…'),
          enabled: _canApprove && !_busy,
          onTap: _add,
        ),
      ],
    );
  }

  Widget _row(PluginInfo p, PluginsMine mine, TextStyle muted) {
    final own = mine.enabled.contains(p.id);
    final inherited = mine.inherited.contains(p.id);
    return SwitchListTile(
      key: ValueKey('employee-plugin-${p.id}'),
      // From the project only: on, and only the project's list can switch it off.
      value: own || inherited,
      onChanged: _canApprove && !_busy && !(inherited && !own) ? (v) => _toggle(p.id, v) : null,
      title: Text(p.version.isEmpty ? p.name : '${p.name} ${p.version}'),
      subtitle: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        if (inherited)
          Padding(
            padding: const EdgeInsets.only(bottom: 2),
            child: Text(own ? 'Also from project' : 'From project', key: ValueKey('plugin-from-project-${p.id}'), style: TextStyle(color: Tokens.accent, fontSize: 12.5, fontWeight: FontWeight.w600)),
          ),
        if (p.description.isNotEmpty) Text(p.description, maxLines: 2, overflow: TextOverflow.ellipsis, style: muted),
        const SizedBox(height: 4),
        PluginChips(p.components),
        PluginNotes(p.components),
      ]),
    );
  }
}
