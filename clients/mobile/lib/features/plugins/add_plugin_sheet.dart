import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import 'plugin_widgets.dart';

/// Add plugin: a link is looked up (`plugin.preview`), or the plugins of the
/// marketplaces already added are browsed (`plugin.list_available`). Pick plugins and
/// the employees that get them, then install. Answers true when something was
/// installed. [employeeId] is pre-ticked. Plugins are no longer given to projects
/// from the app; existing project assignments stay visible on the Plugins page.
Future<bool?> showAddPluginSheet(BuildContext context, {String? employeeId}) {
  final repository = context.read<AureliusRepository>();
  final company = context.read<CompanyBloc>().state;
  final employees = company.employees.where((m) => m.status != LifecycleStatus.archived).toList();
  // Shown under each name: its department.
  String? project(EmployeeSummary m) => company.department(m.departmentId)?.name;
  return showModalBottomSheet<bool>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (sheet) => _AddPluginSheet(
      repository: repository,
      employees: [for (final m in employees) (id: m.id, name: m.name, project: project(m))],
      preselected: employeeId,
    ),
  );
}

typedef _EmployeeChoice = ({String id, String name, String? project});

class _AddPluginSheet extends StatefulWidget {
  const _AddPluginSheet({required this.repository, required this.employees, this.preselected});

  final AureliusRepository repository;
  final List<_EmployeeChoice> employees;
  final String? preselected;

  @override
  State<_AddPluginSheet> createState() => _AddPluginSheetState();
}

class _AddPluginSheetState extends State<_AddPluginSheet> {
  final _link = TextEditingController();
  bool _browse = false;
  bool _busy = false;
  String _progress = '';
  String? _error;
  PluginPreview? _linked;
  PluginCatalog? _available;
  final Set<String> _picked = {};
  late final Set<String> _employees = {if (widget.preselected != null) widget.preselected!};

  bool get _canApprove => widget.repository.linkState.can('approve');
  ({String marketplace, List<PluginOffer> offers})? get _catalog {
    final available = _available;
    final linked = _linked;
    if (_browse) return available == null ? null : (marketplace: '', offers: available.offers);
    return linked == null ? null : (marketplace: linked.marketplace, offers: linked.offers);
  }

  @override
  void dispose() {
    _link.dispose();
    super.dispose();
  }

  Future<void> _guard(String progress, Future<void> Function() body) async {
    setState(() {
      _busy = true;
      _progress = progress;
      _error = null;
    });
    try {
      await body();
    } on ProtocolException catch (e) {
      if (mounted) setState(() => _error = e.isForbidden ? pluginsNeedsApprove : pluginError(e, 'That did not work.'));
    } catch (e) {
      if (mounted) setState(() => _error = pluginError(e, 'That did not work.'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _lookUp() async {
    final source = _link.text.trim();
    if (source.isEmpty) return;
    await _guard('Adding the marketplace and reading its plugins…', () async {
      final catalog = await widget.repository.pluginsPreview(source);
      if (!mounted) return;
      setState(() {
        _linked = catalog;
        _picked.clear();
        if (catalog.offers.isEmpty) _error = 'That link has no plugins.';
      });
    });
  }

  Future<void> _loadBrowse() async {
    setState(() {
      _browse = true;
      _picked.clear();
    });
    if (_available != null) return;
    await _guard('Reading the marketplaces…', () async {
      final catalog = await widget.repository.pluginsAvailable();
      if (mounted) setState(() => _available = catalog);
    });
  }

  Future<void> _install() async {
    final ids = _picked.toList();
    await _guard('Installing ${ids.length} ${ids.length == 1 ? 'plugin' : 'plugins'}… this can take a minute.', () async {
      await widget.repository.pluginsInstall(ids, _employees.toList());
      if (!mounted) return;
      toast(context, ids.length == 1 ? 'Plugin installed' : '${ids.length} plugins installed');
      Navigator.pop(context, true);
    });
  }

  @override
  Widget build(BuildContext context) {
    final muted = TextStyle(color: Tokens.muted, fontSize: 12.5);
    final catalog = _catalog;
    final hooks = [for (final o in catalog?.offers ?? const <PluginOffer>[]) if (_picked.contains(o.id) && (o.components?.hooks ?? false)) o.name];
    return SafeArea(
      child: Padding(
        padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.88),
          child: ListView(
            shrinkWrap: true,
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            children: [
              Text('Add plugin', style: Tokens.title(size: 20)),
              const SizedBox(height: 12),
              SegmentedButton<bool>(
                key: const ValueKey('add-plugin-tabs'),
                segments: const [ButtonSegment(value: false, label: Text('Link')), ButtonSegment(value: true, label: Text('Browse'))],
                selected: {_browse},
                onSelectionChanged: _busy ? null : (s) => s.first ? _loadBrowse() : setState(() => _browse = false),
              ),
              const SizedBox(height: 12),
              if (!_browse) ...[
                TextField(
                  key: const ValueKey('add-plugin-link'),
                  controller: _link,
                  enabled: !_busy,
                  autocorrect: false,
                  keyboardType: TextInputType.url,
                  textInputAction: TextInputAction.go,
                  onSubmitted: (_) => _lookUp(),
                  decoration: const InputDecoration(labelText: 'GitHub link, owner/repo or git URL', border: OutlineInputBorder()),
                ),
                const SizedBox(height: 8),
                Align(
                  alignment: Alignment.centerRight,
                  child: FilledButton.tonal(
                    key: const ValueKey('add-plugin-lookup'),
                    onPressed: _busy || !_canApprove ? null : _lookUp,
                    child: const Text('Look up'),
                  ),
                ),
              ] else
                Text('Plugins of the marketplaces already added on this PC.', style: muted),
              if (!_canApprove) Padding(padding: const EdgeInsets.only(top: 8), child: Text(pluginsNeedsApprove, style: muted)),
              if (_busy)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    const LinearProgressIndicator(key: ValueKey('add-plugin-progress')),
                    const SizedBox(height: 6),
                    Text(_progress, style: muted),
                  ]),
                ),
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: Text(_error!, key: const ValueKey('add-plugin-error'), style: TextStyle(color: Tokens.negative, fontSize: 14)),
                ),
              if (catalog != null && catalog.offers.isNotEmpty) ...[
                const SizedBox(height: 16),
                Text(catalog.marketplace.isEmpty ? 'Plugins' : 'Plugins in ${catalog.marketplace}', style: TextStyle(color: Tokens.label, fontSize: 15, fontWeight: FontWeight.w600)),
                for (final o in catalog.offers)
                  CheckboxListTile(
                    key: ValueKey('offer-${o.id}'),
                    contentPadding: EdgeInsets.zero,
                    controlAffinity: ListTileControlAffinity.leading,
                    value: _picked.contains(o.id),
                    onChanged: _busy ? null : (v) => setState(() => v == true ? _picked.add(o.id) : _picked.remove(o.id)),
                    title: Text(o.installed ? '${o.name} · installed' : o.name),
                    subtitle: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      if (o.description.isNotEmpty) Text(o.description, style: TextStyle(color: Tokens.text2, fontSize: 13)),
                      const SizedBox(height: 4),
                      PluginChips(o.components),
                      PluginNotes(o.components),
                    ]),
                  ),
              ],
              if (catalog != null && catalog.offers.isEmpty && _browse && !_busy && _error == null)
                Padding(padding: const EdgeInsets.only(top: 12), child: Text('No marketplace has been added yet. Use a link first.', style: muted)),
              if (_picked.isNotEmpty) ...[
                const SizedBox(height: 12),
                Text('Use it with', style: TextStyle(color: Tokens.label, fontSize: 15, fontWeight: FontWeight.w600)),
                if (widget.employees.isEmpty) Text('There are no employees yet.', style: muted),
                for (final m in widget.employees)
                  CheckboxListTile(
                    key: ValueKey('employee-${m.id}'),
                    contentPadding: EdgeInsets.zero,
                    controlAffinity: ListTileControlAffinity.leading,
                    value: _employees.contains(m.id),
                    onChanged: _busy ? null : (v) => setState(() => v == true ? _employees.add(m.id) : _employees.remove(m.id)),
                    title: Text(m.name),
                    subtitle: m.project == null ? null : Text(m.project!, style: muted),
                  ),
                if (hooks.isNotEmpty && _employees.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Icon(Icons.warning_amber_rounded, size: 16, color: Tokens.warning),
                      const SizedBox(width: 6),
                      Expanded(
                        child: Text('${hooks.join(', ')} ${hooks.length == 1 ? 'runs' : 'run'} hooks: commands on this PC, outside the approval prompts. Assigning it counts as consent.',
                            style: TextStyle(color: Tokens.warning, fontSize: 13)),
                      ),
                    ]),
                  ),
                const SizedBox(height: 12),
                FilledButton(
                  key: const ValueKey('add-plugin-install'),
                  onPressed: _busy || !_canApprove ? null : _install,
                  child: Text(_employees.isEmpty ? 'Install' : 'Install and use'),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
