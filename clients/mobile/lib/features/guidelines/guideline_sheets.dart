import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/guidelines.dart';
import '../../data/line_diff.dart';
import '../../data/views.dart';
import '../inbox/diff_view.dart';
import 'guideline_common.dart';
import 'guidelines_bloc.dart';

/// The History of a guideline (ui §4.2, §4.3): every version newest first,
/// with who, when and how it came about; a tap shows its diff against the
/// current text or the version before (GL-17 AC3). Restore makes a new
/// version after a question (P-58), for board members.
Future<void> showGuidelineHistory(BuildContext context, GuidelineView g) {
  final bloc = context.read<GuidelinesBloc>();
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (sheet) => BlocProvider.value(value: bloc, child: _History(guideline: g)),
  );
}

class _History extends StatefulWidget {
  const _History({required this.guideline});

  final GuidelineView guideline;

  @override
  State<_History> createState() => _HistoryState();
}

class _HistoryState extends State<_History> {
  late Future<List<GuidelineVersionView>> _versions = context.read<GuidelinesBloc>().repository.guidelineVersionList(widget.guideline.id);
  final _againstPrevious = <int>{};

  Future<void> _restore(GuidelineVersionView v, int newest) async {
    final g = widget.guideline;
    if (!await confirm(context,
        title: 'Restore version ${v.version} of ${g.title}?', body: 'This becomes version ${newest + 1} and applies from the next run.', action: 'Restore')) {
      return;
    }
    if (!mounted) return;
    final bloc = context.read<GuidelinesBloc>();
    final names = Names.of(context);
    try {
      final made = await bloc.restoreVersion(g.id, v.version);
      if (!mounted) return;
      toast(context, 'Restored. Version ${made ?? newest + 1} applies from the next run.');
      setState(() => _versions = bloc.repository.guidelineVersionList(g.id));
    } on Object catch (e) {
      if (mounted) toast(context, names.error(e, title: v.title));
    }
  }

  @override
  Widget build(BuildContext context) {
    final canEdit = context.read<GuidelinesBloc>().canEdit;
    return SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.85),
        child: FutureBuilder<List<GuidelineVersionView>>(
          future: _versions,
          builder: (context, snap) {
            if (snap.hasError) return Padding(padding: const EdgeInsets.all(24), child: ErrorRetry(Names.of(context).error(snap.error!)));
            final versions = snap.data;
            if (versions == null) return const Padding(padding: EdgeInsets.all(32), child: Center(child: CircularProgressIndicator.adaptive()));
            final current = versions.first;
            return ListView(
              key: const ValueKey('guideline-history'),
              shrinkWrap: true,
              children: [
                Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 8), child: Text('History · ${widget.guideline.title}', style: Tokens.title(size: 18))),
                for (final (i, v) in versions.indexed)
                  ExpansionTile(
                    key: ValueKey('version-${v.version}'),
                    title: Text(['Version ${v.version}', if (i == 0) 'current', ?v.by?.username, Fmt.ago(v.createdAtMs)].join(' · ')),
                    subtitle: Text(sourceLabel(v), style: TextStyle(fontSize: 12.5, color: Tokens.label2)),
                    childrenPadding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
                    expandedCrossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      if (i + 1 < versions.length)
                        Align(
                          alignment: Alignment.centerLeft,
                          child: PillSegments<bool>(
                            segments: [(false, i == 0 ? 'Text' : 'Against current'), (true, 'Against version ${versions[i + 1].version}')],
                            selected: _againstPrevious.contains(v.version),
                            onChanged: (prev) => setState(() => prev ? _againstPrevious.add(v.version) : _againstPrevious.remove(v.version)),
                          ),
                        ),
                      const SizedBox(height: 8),
                      if (_againstPrevious.contains(v.version) && i + 1 < versions.length)
                        DiffLines(key: ValueKey('version-diff-${v.version}'), lines: lineDiff(versions[i + 1].text, v.text))
                      else if (i == 0)
                        DiffLines(key: ValueKey('version-diff-${v.version}'), lines: lineDiff('', v.text))
                      else
                        DiffLines(key: ValueKey('version-diff-${v.version}'), lines: lineDiff(v.text, current.text)),
                      if (canEdit && i > 0)
                        Align(
                          alignment: Alignment.centerRight,
                          child: TextButton(key: ValueKey('restore-${v.version}'), onPressed: () => _restore(v, current.version), child: const Text('Restore')),
                        ),
                    ],
                  ),
              ],
            );
          },
        ),
      ),
    );
  }
}

/// Add starter set… (ui §8.2, P-63): the templates with a box each; they
/// become guidelines without links.
Future<void> showStarterSet(BuildContext context) {
  final bloc = context.read<GuidelinesBloc>();
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (sheet) => BlocProvider.value(value: bloc, child: const _StarterSet()),
  );
}

class _StarterSet extends StatefulWidget {
  const _StarterSet();

  @override
  State<_StarterSet> createState() => _StarterSetState();
}

class _StarterSetState extends State<_StarterSet> {
  late final Future<List<StarterTemplate>> _templates = context.read<GuidelinesBloc>().repository.starterTemplates();
  Set<String>? _chosen;
  var _adding = false;

  Future<void> _add() async {
    final bloc = context.read<GuidelinesBloc>();
    final names = Names.of(context);
    setState(() => _adding = true);
    try {
      final n = await bloc.addStarterSet(_chosen!.toList()..sort());
      if (!mounted) return;
      Navigator.pop(context);
      toast(context, 'Added $n guideline${n == 1 ? '' : 's'}. Link them to employees to use them.');
    } on Object catch (e) {
      if (!mounted) return;
      setState(() => _adding = false);
      toast(context, names.error(e));
    }
  }

  @override
  Widget build(BuildContext context) => SafeArea(
        child: FutureBuilder<List<StarterTemplate>>(
          future: _templates,
          builder: (context, snap) {
            if (snap.hasError) return Padding(padding: const EdgeInsets.all(24), child: ErrorRetry(Names.of(context).error(snap.error!)));
            final templates = snap.data;
            if (templates == null) return const Padding(padding: EdgeInsets.all(32), child: Center(child: CircularProgressIndicator.adaptive()));
            final chosen = _chosen ??= {for (final t in templates) if (!t.inLibrary) t.key};
            return ListView(
              key: const ValueKey('starter-set'),
              shrinkWrap: true,
              children: [
                Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Text('Add starter set', style: Tokens.title(size: 18))),
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                  child: Text('They become guidelines without links. Link them to employees or projects to use them.', style: TextStyle(fontSize: 13, color: Tokens.label2)),
                ),
                for (final t in templates)
                  CheckboxListTile(
                    key: ValueKey('starter-${t.key}'),
                    value: t.inLibrary || chosen.contains(t.key),
                    onChanged: t.inLibrary || _adding ? null : (on) => setState(() => on == true ? chosen.add(t.key) : chosen.remove(t.key)),
                    title: Text(t.title),
                    subtitle: Text(
                      t.inLibrary ? 'Already in the library' : '${t.level == GuidelineLevel.project ? 'For a project' : 'For employees'} · ${chars(t.chars)} chars',
                      style: TextStyle(fontSize: 12.5, color: Tokens.label2),
                    ),
                  ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
                  child: FilledButton(
                    key: const ValueKey('starter-add'),
                    onPressed: chosen.isEmpty || _adding ? null : _add,
                    child: Text(chosen.isEmpty ? 'Add' : 'Add ${chosen.length}'),
                  ),
                ),
              ],
            );
          },
        ),
      );
}
