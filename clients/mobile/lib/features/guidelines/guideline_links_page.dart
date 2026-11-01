import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../data/guidelines.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import 'guideline_common.dart';
import 'guidelines_bloc.dart';

/// Who gets a guideline (ui §5.2): Employees, grouped by whom they report to
/// (B2, no departments), and Projects. Each row shows the largest block that
/// employee or project gets with it; a box that would go over 16 000 is off
/// before you tap it (B4). [onSave] null: only choose, and get the choice back
/// (a new guideline's proposal).
class GuidelineLinksPage extends StatefulWidget {
  const GuidelineLinksPage({super.key, this.guidelineId, required this.title, required this.chars, required this.initial, this.onSave});

  final String? guidelineId;
  final String title;

  /// The characters of the guideline's text.
  final int chars;

  /// The links now, as [targetKey]s.
  final Set<String> initial;

  /// Save the new choice; throws the server's refusal.
  final Future<void> Function(Set<String> before, Set<String> after)? onSave;

  /// The new choice, or null when cancelled.
  static Future<Set<String>?> open(BuildContext context, GuidelineLinksPage page) =>
      Navigator.of(context).push<Set<String>>(MaterialPageRoute(fullscreenDialog: true, builder: (_) => withGuidelines(context, page)));

  @override
  State<GuidelineLinksPage> createState() => _GuidelineLinksPageState();
}

enum _Tab { employees, projects }

/// One row's block now and with the guideline: the largest over its contexts.
typedef _Size = ({LargestBlock? now, LargestBlock? with_});

class _GuidelineLinksPageState extends State<GuidelineLinksPage> {
  late final Set<String> _selected = {...widget.initial};
  var _tab = _Tab.employees;
  var _saving = false;
  String? _error;
  final _sizes = <String, _Size>{};

  String get _id => widget.guidelineId ?? '';

  @override
  void initState() {
    super.initState();
    unawaited(_loadSizes());
  }

  Future<void> _loadSizes() async {
    final bloc = context.read<GuidelinesBloc>();
    final company = context.read<CompanyBloc>().state;
    final targets = <GuidelineTarget>[
      for (final e in company.employees) GuidelineTargetEmployee(id: e.id),
      for (final p in company.projects) GuidelineTargetProject(id: p.id),
    ];
    await Future.wait([
      for (final t in targets)
        () async {
          final contexts = contextsOf(t, company.employees, company.projects);
          final without = await bloc.sizes.blocks(contexts, _id, null);
          final key = targetKey(t);
          final linked = widget.initial.contains(key);
          // Now: as the runs are today; with: with this guideline in each context.
          final now = largestOf([
            for (final (ctx, n) in without) (ctx, linked ? n + widget.chars : n),
          ]);
          final with_ = largestOf([for (final (ctx, n) in without) (ctx, n + widget.chars)]);
          if (mounted) setState(() => _sizes[key] = (now: now, with_: with_));
        }(),
    ]);
  }

  /// Why [key] cannot be ticked (it would go over 16 000), or null.
  String? _tooBig(String key, Names names) {
    final big = _sizes[key]?.with_;
    if (big == null || big.chars <= blockMaxChars || _selected.contains(key)) return null;
    return tooLargeLine(
      GuidelineTooLarge(chars: big.chars, limit: blockMaxChars, employeeId: big.context.employeeId, projectId: big.context.projectId),
      employee: names.employee,
      project: names.project,
    ).replaceFirst(' Shorten this text or unlink something.', '');
  }

  String _sizeLine(String key) {
    final s = _sizes[key];
    if (s == null) return '';
    final now = s.now?.chars ?? 0;
    final with_ = s.with_?.chars ?? 0;
    return _selected.contains(key) && !widget.initial.contains(key)
        ? '${chars(now)} → ${chars(with_)}'
        : _selected.contains(key)
            ? '${chars(with_)} / ${chars(blockMaxChars)}'
            : chars(now);
  }

  Future<void> _save() async {
    if (widget.onSave == null) {
      Navigator.pop(context, _selected);
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await widget.onSave!(widget.initial, _selected);
      if (mounted) Navigator.pop(context, _selected);
    } on Object catch (e) {
      // The choice stays, with the server's reason (ui §5.1).
      if (mounted) {
        setState(() {
          _saving = false;
          _error = Names.of(context).error(e);
        });
      }
    }
  }

  Widget _row(String key, String title, String subtitle, Names names, {Key? rowKey}) {
    final reason = _tooBig(key, names);
    final size = _sizeLine(key);
    final big = _sizes[key]?.with_;
    // A ticked row in the colour of its block with the guideline: orange from 80 %, red over 16 000.
    final level = _selected.contains(key) && big != null ? limitLevel(big.chars, blockMaxChars) : LimitLevel.ok;
    final color = reason != null ? Tokens.negative : switch (level) { LimitLevel.ok => Tokens.label2, LimitLevel.close => Tokens.warning, LimitLevel.over => Tokens.negative };
    return CheckboxListTile(
      key: rowKey,
      value: _selected.contains(key),
      onChanged: reason != null || _saving ? null : (on) => setState(() => on == true ? _selected.add(key) : _selected.remove(key)),
      title: Text(title),
      subtitle: Text(
        reason ?? [subtitle, if (size.isNotEmpty) size].where((s) => s.isNotEmpty).join(' · '),
        style: TextStyle(fontSize: 12.5, color: color),
      ),
      controlAffinity: ListTileControlAffinity.trailing,
    );
  }

  @override
  Widget build(BuildContext context) {
    final company = context.watch<CompanyBloc>().state;
    final names = Names(company);
    final groups = reportsGroups(company.employees);
    final projects = [
      ...company.visibleProjects,
      // An archived project's links stay readable and can be undone (GL-06 AC4).
      ...company.projects.where((p) => p.status == ProjectStatus.archived && _selected.contains('p:${p.id}')),
    ];
    final changed = _selected.length != widget.initial.length || !_selected.containsAll(widget.initial);
    return Scaffold(
      appBar: AppBar(
        title: Text('Who gets “${widget.title}”', overflow: TextOverflow.ellipsis),
        leading: IconButton(tooltip: 'Cancel', icon: const Icon(Icons.close), onPressed: () => Navigator.pop(context)),
        actions: [
          Padding(
            padding: const EdgeInsets.only(right: 12),
            child: FilledButton(
              key: const ValueKey('links-save'),
              onPressed: _saving || (!changed && widget.onSave != null) ? null : _save,
              child: Text(widget.onSave == null ? 'Done' : 'Save'),
            ),
          ),
        ],
      ),
      body: SafeArea(
        child: ListView(
          key: const ValueKey('links-list'),
          padding: const EdgeInsets.only(bottom: 24),
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
              child: Align(
                alignment: Alignment.centerLeft,
                child: PillSegments<_Tab>(
                  segments: const [(_Tab.employees, 'Employees'), (_Tab.projects, 'Projects')],
                  selected: _tab,
                  onChanged: (t) => setState(() => _tab = t),
                ),
              ),
            ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
                child: Text(_error!, key: const ValueKey('links-error'), style: TextStyle(color: Tokens.negative, fontSize: 13)),
              ),
            if (_tab == _Tab.employees) ...[
              for (final g in groups) ...[
                _groupHeader(g),
                InsetGroup(children: [
                  for (final e in g.employees)
                    _row('e:${e.id}', '${e.name}${statusMark(e)}', e.roleLine, names, rowKey: ValueKey('link-e-${e.id}')),
                ]),
              ],
              Padding(
                padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
                child: Text(linkGroupNote, style: TextStyle(fontSize: 13, color: Tokens.label2)),
              ),
            ] else ...[
              const GroupHeader('Projects'),
              InsetGroup(children: [
                for (final p in projects)
                  _row('p:${p.id}', p.status == ProjectStatus.archived ? '${p.name} (archived)' : p.name, 'everyone working in this project', names, rowKey: ValueKey('link-p-${p.id}')),
              ]),
              Padding(
                padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
                child: Text('A project link applies to every run in that project, whoever does it.', style: TextStyle(fontSize: 13, color: Tokens.label2)),
              ),
            ],
          ],
        ),
      ),
    );
  }

  /// "REPORTS TO TITAN · 4 of 7  [All]": All ticks the group's current
  /// members that fit, None unticks them (P-54).
  Widget _groupHeader(ReportsGroup g) {
    final keys = [for (final e in g.employees) 'e:${e.id}'];
    final on = keys.where(_selected.contains).length;
    final all = on == keys.length;
    final names = Names.of(context);
    return Semantics(
      label: '${g.title}, $on of ${keys.length} selected',
      child: GroupHeader(
        '${g.title} · $on of ${keys.length}',
        key: ValueKey('links-group-${g.managerId ?? 'top'}'),
        action: all ? 'None' : 'All',
        onAction: _saving
            ? null
            : () => setState(() {
                  if (all) {
                    _selected.removeAll(keys);
                  } else {
                    _selected.addAll(keys.where((k) => _tooBig(k, names) == null));
                  }
                }),
      ),
    );
  }
}
