import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/guidelines.dart';
import '../../data/views.dart';
import 'guideline_common.dart';
import 'guideline_editor_page.dart';
import 'guideline_links_page.dart';
import 'guideline_markdown.dart';
import 'guideline_proposal_card.dart';
import 'guideline_sheets.dart';
import 'guidelines_bloc.dart';

/// Open one guideline; [version]: that version only, read-only (a run's, GL-11 AC3).
Future<void> openGuideline(BuildContext context, String guidelineId, {int? version}) =>
    Navigator.of(context).push<void>(MaterialPageRoute(builder: (_) => withGuidelines(context, GuidelinePage(guidelineId: guidelineId, version: version))));

/// One guideline to read (GL-17 AC1): the rendered text (E14), its version,
/// who it applies to and from when (B6), a pending proposal on top, and for
/// board members Edit, Links, History, Archive or Restore.
class GuidelinePage extends StatefulWidget {
  const GuidelinePage({super.key, required this.guidelineId, this.version});

  final String guidelineId;
  final int? version;

  @override
  State<GuidelinePage> createState() => _GuidelinePageState();
}

class _GuidelinePageState extends State<GuidelinePage> {
  GuidelineDetail? _detail;
  GuidelineAffected? _reach;
  String? _error;
  StreamSubscription<void>? _changes;

  @override
  void initState() {
    super.initState();
    final repo = context.read<GuidelinesBloc>().repository;
    _changes = repo.guidelineChanges.where((e) => e.aggregateId == widget.guidelineId).listen((_) => _load());
    unawaited(_load());
  }

  @override
  void dispose() {
    unawaited(_changes?.cancel());
    super.dispose();
  }

  Future<void> _load() async {
    final repo = context.read<GuidelinesBloc>().repository;
    try {
      final detail = await repo.guideline(widget.guidelineId, version: widget.version);
      GuidelineAffected? reach;
      try {
        reach = await repo.guidelineReach(widget.guidelineId);
      } on Object {
        reach = null;
      }
      if (!mounted) return;
      setState(() {
        _detail = detail;
        _reach = reach;
        _error = null;
      });
    } on Object catch (e) {
      if (mounted) setState(() => _error = Names.of(context).error(e));
    }
  }

  Future<void> _edit(GuidelineDetail d) async {
    final bloc = context.read<GuidelinesBloc>();
    final g = d.guideline;
    final saved = await GuidelineEditorPage.open(
      context,
      GuidelineEditorPage(
        guideline: g,
        fromVersion: d.version.version,
        initialTitle: d.version.title,
        initialText: d.version.text,
        links: g.links,
        onSave: (title, text) => bloc.update(
          g,
          title: title == d.version.title ? null : title,
          text: text == d.version.text ? null : text,
          fromVersion: d.version.version,
        ),
      ),
    );
    if (saved && mounted) {
      toast(context, 'Saved. ${appliesLine(_reach)}');
      unawaited(_load());
    }
  }

  Future<void> _links(GuidelineDetail d) async {
    final bloc = context.read<GuidelinesBloc>();
    final g = d.guideline;
    final saved = await GuidelineLinksPage.open(
      context,
      GuidelineLinksPage(
        guidelineId: g.id,
        title: g.title,
        chars: g.chars,
        initial: {for (final l in g.links) targetKey(l)},
        onSave: (before, after) => bloc.setLinks(g.id, before, after),
      ),
    );
    if (saved == null || !mounted) return;
    await _load();
    if (mounted) toast(context, 'Links saved. ${appliesLine(_reach)}');
  }

  Future<void> _archive(GuidelineDetail d) async {
    final bloc = context.read<GuidelinesBloc>();
    final names = Names.of(context);
    final g = d.guideline;
    if (!await confirm(context, title: 'Archive ${g.title}?', body: archiveBody(g, names), action: 'Archive', danger: true)) return;
    try {
      await bloc.archive(g.id);
      if (mounted) toast(context, 'Archived. No new run gets ${g.title}.');
      unawaited(_load());
    } on Object catch (e) {
      if (mounted) toast(context, names.error(e, title: g.title));
    }
  }

  Future<void> _restore(GuidelineDetail d) async {
    final bloc = context.read<GuidelinesBloc>();
    final names = Names.of(context);
    try {
      await bloc.restore(d.guideline.id);
      if (mounted) toast(context, 'Restored. Its links apply again from the next run.');
      unawaited(_load());
    } on Object catch (e) {
      if (mounted) toast(context, names.error(e, title: d.guideline.title));
    }
  }

  @override
  Widget build(BuildContext context) {
    final d = _detail;
    final bloc = context.watch<GuidelinesBloc>();
    final names = Names.of(context);
    final canEdit = bloc.canEdit && widget.version == null;
    final archived = d?.guideline.archived ?? false;
    final pending = bloc.state.proposalsFor(widget.guidelineId);
    return Scaffold(
      appBar: GlassAppBar(
        title: Text(d?.version.title ?? 'Guideline', overflow: TextOverflow.ellipsis),
        actions: [
          if (d != null && canEdit)
            GlassMenuButton<String>(
              tooltip: 'More',
              items: () => [
                if (!archived) const GlassMenuItem('edit', 'Edit'),
                if (!archived) const GlassMenuItem('links', 'Links…'),
                const GlassMenuItem('history', 'History'),
                archived ? const GlassMenuItem('restore', 'Restore') : const GlassMenuItem('archive', 'Archive', danger: true),
              ],
              onSelected: (v) => switch (v) {
                'edit' => _edit(d),
                'links' => _links(d),
                'history' => showGuidelineHistory(context, d.guideline),
                'restore' => _restore(d),
                _ => _archive(d),
              },
            ),
        ],
      ),
      body: d == null
          ? (_error != null ? ErrorRetry(_error!, onRetry: _load) : const Center(child: CircularProgressIndicator.adaptive()))
          : ListView(
              key: const ValueKey('guideline-page'),
              padding: EdgeInsets.only(bottom: listBottom(context)),
              children: [
                for (final p in pending)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
                    child: Material(
                      color: Tokens.accentTint,
                      borderRadius: BorderRadius.circular(Tokens.radius),
                      child: ListTile(
                        key: ValueKey('guideline-proposal-bar-${p.id}'),
                        leading: Icon(CupertinoIcons.sparkles, color: Tokens.accent),
                        title: Text('${names.employee(p.proposedBy)} proposes a change'),
                        trailing: const Text('Review ›'),
                        onTap: () => openGuidelineProposal(context, p.id),
                      ),
                    ),
                  ),
                if (archived)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
                    child: Text('Archived. No run gets it; its versions and links stay readable.', key: const ValueKey('guideline-archived'), style: TextStyle(color: Tokens.warning, fontSize: 13)),
                  ),
                if (widget.version != null && widget.version != d.guideline.version)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
                    child: Text('Version ${widget.version}, as a run had it. The current version is ${d.guideline.version}.', style: TextStyle(color: Tokens.label2, fontSize: 13)),
                  ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(32, 12, 32, 4),
                  child: Text(
                    ['Version ${d.version.version}', '${chars(charCount(d.version.text))} chars', ?d.version.by?.username, sourceLabel(d.version), Fmt.ago(d.version.createdAtMs)].join(' · '),
                    key: const ValueKey('guideline-meta'),
                    style: TextStyle(fontSize: 13, color: Tokens.label2),
                  ),
                ),
                InsetGroup(children: [
                  GroupRow(
                    key: const ValueKey('guideline-applies-to'),
                    title: 'Applies to',
                    subtitle: linksLine(d.guideline.links, employee: names.employee, project: names.project, shown: 6),
                    onTap: canEdit && !archived ? () => _links(d) : null,
                    chevron: canEdit && !archived,
                  ),
                  GroupRow(key: const ValueKey('guideline-history-row'), title: 'History', onTap: () => showGuidelineHistory(context, d.guideline)),
                ]),
                if (!archived)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(32, 8, 32, 0),
                    child: Text(appliesLine(_reach), key: const ValueKey('guideline-reach'), style: TextStyle(fontSize: 13, color: Tokens.label2)),
                  ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(20, 16, 20, 0),
                  child: SurfaceCard(child: GuidelineMarkdown(d.version.text)),
                ),
              ],
            ),
    );
  }
}

/// What archiving takes away: the links it had (GL-04 AC1).
String archiveBody(GuidelineView g, Names names) => g.links.isEmpty
    ? 'It is not linked. Its versions stay; you can restore it later.'
    : 'No new run gets it any more. It is linked to ${linksLine(g.links, employee: names.employee, project: names.project, shown: 8)}. Its versions stay; you can restore it later.';
