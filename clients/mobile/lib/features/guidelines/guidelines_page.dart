import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/guidelines.dart';
import '../../data/views.dart';
import '../connection/connection_bloc.dart';
import '../inbox/inbox_bloc.dart';
import 'guideline_common.dart';
import 'guideline_editor_page.dart';
import 'guideline_page.dart';
import 'guideline_proposal_card.dart';
import 'guideline_sheets.dart';
import 'guidelines_bloc.dart';

/// More → Guidelines (ui §3, GL-17): the library with search, a row per
/// guideline (version, links, size, ◆ for a pending proposal), the proposals
/// for new guidelines, the archived ones apart, the seed banner and the empty
/// state. Board members add, edit, link and archive; the rest reads.
class GuidelinesPage extends StatelessWidget {
  const GuidelinesPage({super.key});

  @override
  Widget build(BuildContext context) {
    if (!context.select<ConnectionBloc, bool>((b) => b.state.has('guidelines'))) {
      return const LargeTitleScaffold(title: 'Guidelines', fill: EmptyState('No guidelines on this server yet', detail: "The server can't do this yet. Update Aurelius on the server."));
    }
    return BlocProvider(
      create: (context) => GuidelinesBloc(context.read<AureliusRepository>()),
      child: const _GuidelinesView(),
    );
  }
}

/// A new guideline in the editor; it opens once saved.
Future<void> newGuideline(BuildContext context) async {
  final bloc = context.read<GuidelinesBloc>();
  String? made;
  final saved = await GuidelineEditorPage.open(context, GuidelineEditorPage(onSave: (title, text) async => made = await bloc.create(title, text)));
  if (!saved || !context.mounted) return;
  toast(context, 'Saved as version 1. Link it to employees or projects to use it.');
  if (made != null) await openGuideline(context, made!);
}

class _GuidelinesView extends StatelessWidget {
  const _GuidelinesView();

  Future<void> _archive(BuildContext context, GuidelineView g) async {
    final bloc = context.read<GuidelinesBloc>();
    final names = Names.of(context);
    try {
      await bloc.archive(g.id);
      if (context.mounted) toast(context, 'Archived. No new run gets ${g.title}.');
    } on Object catch (e) {
      if (context.mounted) toast(context, names.error(e, title: g.title));
    }
  }

  @override
  Widget build(BuildContext context) {
    final state = context.watch<GuidelinesBloc>().state;
    final bloc = context.read<GuidelinesBloc>();
    final names = Names.of(context);
    final canEdit = bloc.canEdit;
    final newOnes = context.select<InboxBloc, List<GuidelineProposalItem>>((b) => b.state.guidelineProposals.where((g) => g.isNew).toList());
    final nothing = state.empty && state.archived.isEmpty && newOnes.isEmpty;
    return BlocListener<GuidelinesBloc, GuidelinesState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, s) {
        toast(context, s.notice!);
        bloc.add(const GuidelinesNoticeShown());
      },
      child: LargeTitleScaffold(
        title: 'Guidelines',
        actions: [
          if (canEdit) GlassButton(key: const ValueKey('guideline-new'), tooltip: 'New guideline', icon: CupertinoIcons.add, onPressed: () => newGuideline(context)),
        ],
        onRefresh: () async => bloc.add(const GuidelinesRefreshed()),
        fill: state.active == null
            ? (state.error != null ? ErrorRetry(state.error!, onRetry: () => bloc.add(const GuidelinesRefreshed())) : const Center(child: CircularProgressIndicator.adaptive()))
            : nothing
                ? _EmptyLibrary(canEdit: canEdit)
                : null,
        children: state.active == null || nothing
            ? null
            : [
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
                  child: CupertinoSearchTextField(key: const ValueKey('guideline-search'), onChanged: (q) => bloc.add(GuidelinesQueryChanged(q))),
                ),
                if (state.showSeedBanner) const _SeedBanner(),
                if (state.empty) _EmptyLibrary(canEdit: canEdit, inline: true),
                if (state.shown.isNotEmpty)
                  InsetGroup(children: [
                    for (final g in state.shown)
                      canEdit
                          ? Dismissible(
                              key: ValueKey('guideline-swipe-${g.id}'),
                              direction: DismissDirection.endToStart,
                              background: Container(
                                alignment: Alignment.centerRight,
                                padding: const EdgeInsets.only(right: 20),
                                color: Tokens.negative,
                                child: const Text('Archive', style: TextStyle(color: Colors.white, fontWeight: FontWeight.w600)),
                              ),
                              confirmDismiss: (_) => confirm(context, title: 'Archive ${g.title}?', body: archiveBody(g, names), action: 'Archive', danger: true),
                              onDismissed: (_) => _archive(context, g),
                              child: GuidelineRow(guideline: g),
                            )
                          : GuidelineRow(guideline: g),
                  ])
                else if (!state.empty)
                  Padding(padding: const EdgeInsets.fromLTRB(32, 8, 32, 8), child: Text('No guideline matches.', style: TextStyle(color: Tokens.label2))),
                if (newOnes.isNotEmpty) ...[
                  GroupHeader('Proposed', count: newOnes.length, info: 'New guidelines employees proposed. A board member approves or rejects them.'),
                  InsetGroup(children: [
                    for (final p in newOnes)
                      GroupRow(
                        key: ValueKey('guideline-proposed-${p.id}'),
                        leading: const NeedsYouTag('◆'),
                        title: p.proposal.title,
                        subtitle: 'Proposed by ${names.employee(p.proposal.proposedBy)} · ${chars(charCount(p.proposal.text))} chars',
                        onTap: () => openGuidelineProposal(context, p.id),
                      ),
                  ]),
                ],
                if (state.shownArchived.isNotEmpty) ...[
                  GroupHeader('Archived', key: const ValueKey('guideline-archived-header'), count: state.shownArchived.length, info: 'No run gets these. Their versions and links stay; open one to restore it.'),
                  InsetGroup(children: [for (final g in state.shownArchived) GuidelineRow(guideline: g)]),
                ],
                Padding(
                  padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
                  child: Text('Size in characters (max ${chars(guidelineMaxChars)} each, ${chars(blockMaxChars)} per run).', style: TextStyle(fontSize: 13, color: Tokens.label2)),
                ),
              ],
      ),
    );
  }
}

/// One guideline: title, ◆ for each pending proposal, version, links and size.
class GuidelineRow extends StatelessWidget {
  const GuidelineRow({super.key, required this.guideline});

  final GuidelineView guideline;

  @override
  Widget build(BuildContext context) {
    final g = guideline;
    final names = Names.of(context);
    return GroupRow(
      key: ValueKey('guideline-${g.id}'),
      title: g.title,
      subtitle: librarySubtitle(g, employee: names.employee, project: names.project),
      trailing: Row(mainAxisSize: MainAxisSize.min, children: [
        if (g.openProposals > 0) Padding(padding: const EdgeInsets.only(right: 8), child: NeedsYouTag('◆ ${g.openProposals}')),
        Text('v${g.version}', style: TextStyle(fontSize: 15, color: Tokens.label2)),
        const Padding(padding: EdgeInsets.only(left: 6), child: Chevron()),
      ]),
      onTap: () => openGuideline(context, g.id),
    );
  }
}

/// After the seed (B7, ui §8.1), once per board member until Got it.
class _SeedBanner extends StatelessWidget {
  const _SeedBanner();

  @override
  Widget build(BuildContext context) => Padding(
        key: const ValueKey('guideline-seed-banner'),
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
        child: SurfaceCard(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            const Text(seedBannerText, style: TextStyle(fontSize: 14)),
            Align(
              alignment: Alignment.centerRight,
              child: TextButton(key: const ValueKey('guideline-seed-got-it'), onPressed: () => context.read<GuidelinesBloc>().add(const GuidelinesBannerDismissed()), child: const Text('Got it')),
            ),
          ]),
        ),
      );
}

/// No guidelines yet (ui §8.2): what they are, New guideline and Add starter set.
class _EmptyLibrary extends StatelessWidget {
  const _EmptyLibrary({required this.canEdit, this.inline = false});

  final bool canEdit;
  final bool inline;

  @override
  Widget build(BuildContext context) {
    final body = Padding(
      key: const ValueKey('guideline-empty'),
      padding: const EdgeInsets.symmetric(horizontal: 32, vertical: 24),
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        Text(emptyLibraryTitle, style: Tokens.title(size: 20), textAlign: TextAlign.center),
        const SizedBox(height: 8),
        Text(emptyLibraryDetail, textAlign: TextAlign.center, style: TextStyle(fontSize: 15, color: Tokens.label2)),
        if (canEdit) ...[
          const SizedBox(height: 16),
          FilledButton(key: const ValueKey('guideline-empty-new'), onPressed: () => newGuideline(context), child: const Text('New guideline')),
          TextButton(key: const ValueKey('guideline-empty-starter'), onPressed: () => showStarterSet(context), child: const Text('Add starter set')),
        ],
        const SizedBox(height: 8),
        Text(emptyLibraryProposeHint, textAlign: TextAlign.center, style: TextStyle(fontSize: 13, color: Tokens.label2)),
      ]),
    );
    return inline ? body : Center(child: SingleChildScrollView(child: body));
  }
}
