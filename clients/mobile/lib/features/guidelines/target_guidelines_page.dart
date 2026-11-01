import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/guidelines.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import 'guideline_common.dart';
import 'guideline_page.dart';
import 'guidelines_bloc.dart';

/// The row on an employee's or a project's page (ui §6.3): "3 · 9 110 / 16 000
/// characters" for an employee (its largest block), "2 · +3 830 characters per
/// run" for a project. Nothing on a server without guidelines.
class GuidelinesRow extends StatefulWidget {
  const GuidelinesRow({super.key, required this.target});

  final GuidelineTarget target;

  @override
  State<GuidelinesRow> createState() => _GuidelinesRowState();
}

class _GuidelinesRowState extends State<GuidelinesRow> {
  Future<String>? _subtitle;

  @override
  void initState() {
    super.initState();
    final repo = context.read<AureliusRepository>();
    if (guidelinesOn(context)) _subtitle = _read(repo);
  }

  Future<String> _read(AureliusRepository repo) async {
    switch (widget.target) {
      case GuidelineTargetProject(:final id):
        final set = await repo.effectiveGuidelines(projectId: id);
        return '${set.guidelines.length} · +${chars(set.totalChars)} characters per run';
      case GuidelineTargetEmployee(:final id):
        final employee = context.read<CompanyBloc>().state.employee(id);
        final sets = await Future.wait([
          for (final ctx in workContextsOf(employee, id)) repo.effectiveGuidelines(employeeId: ctx.employeeId, projectId: ctx.projectId),
        ]);
        final own = sets.first.guidelines.length;
        final largest = sets.fold<int>(0, (m, s) => s.totalChars > m ? s.totalChars : m);
        return '$own · ${counterLine(largest, max: blockMaxChars)} characters';
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_subtitle == null) return const SizedBox.shrink();
    return FutureBuilder<String>(
      future: _subtitle,
      builder: (context, snap) => GroupRow(
        key: const ValueKey('target-guidelines-row'),
        title: 'Guidelines',
        subtitle: snap.data ?? (snap.hasError ? 'Could not read them' : ' '),
        onTap: () => Navigator.of(context).push<void>(MaterialPageRoute(builder: (_) => withGuidelines(context, TargetGuidelinesPage(target: widget.target)))),
      ),
    );
  }
}

/// What a run of an employee (per project it works in, GL-07) or every run in
/// a project gets, in block order with why each is there, against 16 000;
/// Manage (board members) links and unlinks from this side (GL-05 AC2, GL-06 AC2).
class TargetGuidelinesPage extends StatefulWidget {
  const TargetGuidelinesPage({super.key, required this.target});

  final GuidelineTarget target;

  @override
  State<TargetGuidelinesPage> createState() => _TargetGuidelinesPageState();
}

class _TargetGuidelinesPageState extends State<TargetGuidelinesPage> {
  late List<BlockContext> _contexts;
  late BlockContext _shown;
  final _sets = <BlockContext, EffectiveGuidelines>{};
  String? _error;
  StreamSubscription<void>? _changes;

  @override
  void initState() {
    super.initState();
    final bloc = context.read<GuidelinesBloc>();
    _contexts = switch (widget.target) {
      // An employee: without a project, then each project it works in.
      GuidelineTargetEmployee(:final id) => workContextsOf(context.read<CompanyBloc>().state.employee(id), id),
      GuidelineTargetProject(:final id) => [(employeeId: null, projectId: id)],
    };
    _shown = _contexts.first;
    _changes = bloc.repository.guidelineChanges.listen((_) {
      bloc.sizes.clear();
      unawaited(_load());
    });
    unawaited(_load());
  }

  @override
  void dispose() {
    unawaited(_changes?.cancel());
    super.dispose();
  }

  Future<void> _load() async {
    final sizes = context.read<GuidelinesBloc>().sizes;
    try {
      final sets = await Future.wait(_contexts.map(sizes.of));
      if (!mounted) return;
      setState(() {
        for (final (i, ctx) in _contexts.indexed) {
          _sets[ctx] = sets[i];
        }
        _error = null;
      });
    } on Object catch (e) {
      if (mounted) setState(() => _error = Names.of(context).error(e));
    }
  }

  String get _name {
    final names = Names.of(context);
    return switch (widget.target) {
      GuidelineTargetEmployee(:final id) => names.employee(id),
      GuidelineTargetProject(:final id) => names.project(id),
    };
  }

  Future<void> _manage() async {
    final bloc = context.read<GuidelinesBloc>();
    final saved = await Navigator.of(context).push<bool>(MaterialPageRoute(
      fullscreenDialog: true,
      builder: (_) => BlocProvider.value(value: bloc, child: _ManageLinks(target: widget.target, name: _name, contexts: _contexts, sets: Map.of(_sets))),
    ));
    if (saved == true && mounted) {
      toast(context, 'Links saved. They apply from the next run.');
      unawaited(_load());
    }
  }

  @override
  Widget build(BuildContext context) {
    final names = Names.of(context);
    final canEdit = context.read<GuidelinesBloc>().canEdit;
    final set = _sets[_shown];
    final project = widget.target is GuidelineTargetProject;
    return Scaffold(
      appBar: GlassAppBar(
        title: Text('Guidelines · $_name', overflow: TextOverflow.ellipsis),
        actions: [if (canEdit) TextButton(key: const ValueKey('target-guidelines-manage'), onPressed: _sets.length == _contexts.length ? _manage : null, child: const Text('Manage'))],
      ),
      body: set == null
          ? (_error != null ? ErrorRetry(_error!, onRetry: _load) : const Center(child: CircularProgressIndicator.adaptive()))
          : ListView(
              key: const ValueKey('target-guidelines'),
              padding: EdgeInsets.only(bottom: listBottom(context)),
              children: [
                if (_contexts.length > 1)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
                    child: PillSegments<BlockContext>(
                      scrollable: true,
                      segments: [for (final c in _contexts) (c, c.projectId == null ? 'No project' : names.project(c.projectId!))],
                      selected: _shown,
                      onChanged: (c) => setState(() => _shown = c),
                    ),
                  ),
                GroupHeader(project ? 'For everyone in $_name' : (_shown.projectId == null ? 'A run without a project' : 'A run in ${names.project(_shown.projectId!)}'), count: set.guidelines.length),
                InsetGroup(children: [
                  if (set.guidelines.isEmpty)
                    GroupRow(title: project ? 'No guidelines for this project yet.' : 'No guidelines yet. $_name works with its instructions only.', chevron: false),
                  for (final (i, g) in set.guidelines.indexed)
                    GroupRow(
                      key: ValueKey('target-guideline-${g.id}'),
                      leading: Text('${i + 1}', style: Tokens.monoSmall()),
                      title: '${g.title} v${g.version}',
                      subtitle: '${g.level == GuidelineLevel.employee ? 'linked to $_name' : 'from ${names.project(_shown.projectId ?? '')}'} · ${chars(g.chars)} chars',
                      onTap: () => openGuideline(context, g.id),
                    ),
                ]),
                if (set.dropped.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(32, 8, 32, 0),
                    child: Text('Left out of a run now: ${set.dropped.length} guideline${set.dropped.length == 1 ? '' : 's'}; together they are over the limit.', style: TextStyle(color: Tokens.negative, fontSize: 13)),
                  ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
                  child: LimitBar(key: const ValueKey('target-guidelines-block'), count: set.totalChars, max: set.limitChars, label: project ? 'Added to each run here: ' : 'Block: '),
                ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
                  child: Text('Instructions, task, agent rules and permissions always go before these. A project guideline wins over an employee one.', style: TextStyle(fontSize: 13, color: Tokens.label2)),
                ),
              ],
            ),
    );
  }
}

/// Manage from the employee's or project's side (ui §6.3): every active
/// guideline with a box; one that would push a block over 16 000 is off.
class _ManageLinks extends StatefulWidget {
  const _ManageLinks({required this.target, required this.name, required this.contexts, required this.sets});

  final GuidelineTarget target;
  final String name;
  final List<BlockContext> contexts;
  final Map<BlockContext, EffectiveGuidelines> sets;

  @override
  State<_ManageLinks> createState() => _ManageLinksState();
}

class _ManageLinksState extends State<_ManageLinks> {
  Set<String>? _initial;
  Set<String>? _selected;
  var _saving = false;
  String? _error;

  String get _key => targetKey(widget.target);

  /// The largest block with [selected]: the sets now, minus what was unticked, plus what was ticked.
  int _largest(List<GuidelineView> all, Set<String> selected) {
    var largest = 0;
    for (final ctx in widget.contexts) {
      final set = widget.sets[ctx];
      if (set == null) continue;
      final inSet = {for (final g in set.guidelines) g.id};
      var n = set.totalChars;
      for (final g in all) {
        final was = _initial!.contains(g.id);
        final now = selected.contains(g.id);
        // Only what this target's own link brings; a guideline that comes in another way stays.
        if (now && !was && !inSet.contains(g.id)) n += g.chars;
        if (!now && was && inSet.contains(g.id) && g.links.length == 1) n -= g.chars;
      }
      if (n > largest) largest = n;
    }
    return largest;
  }

  Future<void> _save(List<GuidelineView> all) async {
    final bloc = context.read<GuidelinesBloc>();
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      for (final g in all) {
        final was = _initial!.contains(g.id);
        final now = _selected!.contains(g.id);
        if (was == now) continue;
        final before = {for (final l in g.links) targetKey(l)};
        await bloc.setLinks(g.id, before, now ? {...before, _key} : ({...before}..remove(_key)));
      }
      if (mounted) Navigator.pop(context, true);
    } on Object catch (e) {
      if (mounted) {
        setState(() {
          _saving = false;
          _error = Names.of(context).error(e);
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final all = context.watch<GuidelinesBloc>().state.active;
    if (all != null) {
      _initial ??= {for (final g in all) if (g.links.any((l) => targetKey(l) == _key)) g.id};
      _selected ??= {..._initial!};
    }
    final selected = _selected;
    final largest = all == null ? 0 : _largest(all, selected!);
    final changed = selected != null && (selected.length != _initial!.length || !selected.containsAll(_initial!));
    return Scaffold(
      appBar: AppBar(
        title: Text('Guidelines for ${widget.name}', overflow: TextOverflow.ellipsis),
        leading: IconButton(tooltip: 'Cancel', icon: const Icon(Icons.close), onPressed: () => Navigator.pop(context, false)),
        actions: [
          Padding(
            padding: const EdgeInsets.only(right: 12),
            child: FilledButton(key: const ValueKey('manage-save'), onPressed: _saving || !changed || all == null ? null : () => _save(all), child: const Text('Save')),
          ),
        ],
      ),
      body: all == null
          ? const Center(child: CircularProgressIndicator.adaptive())
          : ListView(
              children: [
                if (_error != null) Padding(padding: const EdgeInsets.fromLTRB(16, 8, 16, 0), child: Text(_error!, key: const ValueKey('manage-error'), style: TextStyle(color: Tokens.negative, fontSize: 13))),
                Padding(
                  padding: const EdgeInsets.fromLTRB(32, 12, 32, 4),
                  child: LimitBar(count: largest, max: blockMaxChars, label: 'Largest block: '),
                ),
                InsetGroup(children: [
                  for (final g in all)
                    Builder(builder: (context) {
                      final on = selected!.contains(g.id);
                      final over = !on && _largest(all, {...selected, g.id}) > blockMaxChars;
                      return CheckboxListTile(
                        key: ValueKey('manage-${g.id}'),
                        value: on,
                        onChanged: over || _saving ? null : (v) => setState(() => v == true ? selected.add(g.id) : selected.remove(g.id)),
                        title: Text('${g.title} v${g.version}'),
                        subtitle: Text(
                          over ? 'Would give a block over ${chars(blockMaxChars)} characters' : '${chars(g.chars)} chars',
                          style: TextStyle(fontSize: 12.5, color: over ? Tokens.negative : Tokens.label2),
                        ),
                      );
                    }),
                ]),
              ],
            ),
    );
  }
}
