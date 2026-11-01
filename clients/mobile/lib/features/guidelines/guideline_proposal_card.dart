import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/connection.dart' show LinkState;
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/guidelines.dart';
import '../../data/line_diff.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../inbox/diff_view.dart';
import '../inbox/inbox_bloc.dart';
import 'guideline_common.dart';
import 'guideline_editor_page.dart';
import 'guideline_links_page.dart';
import 'guidelines_bloc.dart';

/// Only a signed-in board member with the Approve right decides (B5, GL-14 AC6).
bool canDecideGuidelines(LinkState link) => link.user != null && link.can('approve');

/// What the board member chose on a card.
typedef GuidelineDecision = void Function(bool approve, {String? note, String? title, String? text, List<GuidelineTarget>? links});

/// A guideline an employee proposed (ui §7.2): the kind, the guideline, who
/// and from which version, the reason and the evidence, the diff against the
/// base version (all `+` for a new one, E14: plain text, nothing rendered),
/// the counter, the largest block a run gets after approving (B4), and for a
/// new guideline its links. Reject (with a note), Edit (approve an edited
/// text) and Approve, for board members only; a stale one can only be
/// rejected, and Approve is off over 8 000 or over 16 000 per run.
class GuidelineProposalCard extends StatefulWidget {
  const GuidelineProposalCard({super.key, required this.item, required this.proposer, required this.canDecide, this.deciding = false, required this.onDecide});

  final GuidelineProposalItem item;
  final String proposer;
  final bool canDecide;
  final bool deciding;
  final GuidelineDecision onDecide;

  @override
  State<GuidelineProposalCard> createState() => _GuidelineProposalCardState();
}

class _GuidelineProposalCardState extends State<GuidelineProposalCard> {
  /// A new guideline's links: the suggested ones until changed (P-61).
  late Set<String> _links = {for (final l in widget.item.proposal.suggestedLinks) targetKey(l)};

  List<GuidelineTarget> get _targets => [for (final k in _links) targetOf(k)];

  /// The block sizes per context, read for this card.
  late final _sizes = GuidelineSizes(context.read<AureliusRepository>());

  /// The largest block a run gets with this proposal approved (null: not read yet, or no links).
  LargestBlock? _largest;

  @override
  void initState() {
    super.initState();
    unawaited(_loadLargest());
  }

  @override
  void didUpdateWidget(GuidelineProposalCard old) {
    super.didUpdateWidget(old);
    if (old.item != widget.item) {
      _sizes.clear();
      unawaited(_loadLargest());
    }
  }

  /// Where the approved guideline applies: the chosen links of a new one, the links of a changed one.
  List<GuidelineTarget> get _appliesTo => widget.item.isNew ? _targets : widget.item.current?.links ?? const [];

  Future<void> _loadLargest() async {
    final p = widget.item.proposal;
    final links = _appliesTo;
    if (links.isEmpty) {
      if (mounted) setState(() => _largest = null);
      return;
    }
    final company = context.read<CompanyBloc>().state;
    final blocks = await _sizes.blocks(contextsOfAll(links, company.employees, company.projects), p.guidelineId ?? '', charCount(p.text));
    if (mounted) setState(() => _largest = largestOf(blocks));
  }

  Future<void> _reject() async {
    final note = await promptForm(context, title: 'Reject this guideline?', action: 'Reject', fields: [FieldSpec('Note for ${widget.proposer}', required: false, lines: 2)]);
    if (note == null) return;
    widget.onDecide(false, note: note.first.isEmpty ? null : note.first);
  }

  Future<void> _edit() async {
    final p = widget.item.proposal;
    String? title;
    String? text;
    final approved = await GuidelineEditorPage.open(
      context,
      GuidelineEditorPage(
        guideline: widget.item.current,
        fromVersion: p.baseVersion,
        initialTitle: p.title,
        initialText: p.text,
        links: widget.item.isNew ? _targets : widget.item.current?.links ?? const [],
        action: 'Approve',
        allowUnchanged: true,
        onSave: (t, x) async => (title, text) = (t, x),
      ),
    );
    if (!approved) return;
    widget.onDecide(true, title: title, text: text, links: widget.item.isNew ? _targets : null);
  }

  Future<void> _chooseLinks() async {
    final p = widget.item.proposal;
    final chosen = await GuidelineLinksPage.open(context, GuidelineLinksPage(title: p.title, chars: charCount(p.text), initial: _links));
    if (chosen != null && mounted) {
      setState(() => _links = chosen);
      unawaited(_loadLargest());
    }
  }

  @override
  Widget build(BuildContext context) {
    final item = widget.item;
    final p = item.proposal;
    final names = Names.of(context);
    final count = charCount(p.text);
    final over = count > guidelineMaxChars;
    final stale = item.stale;
    final current = item.current;
    final largest = _largest;
    // Over 16 000 per run the server would refuse: say so before Approve, as the desktop does.
    final tooLarge = largest != null && largest.chars > blockMaxChars
        ? '${tooLargeLine(GuidelineTooLarge(chars: largest.chars, limit: blockMaxChars, employeeId: largest.context.employeeId, projectId: largest.context.projectId), employee: names.employee, project: names.project).replaceFirst(' Shorten this text or unlink something.', '')} Approve is off; Edit lets you shorten it.'
        : null;
    return SurfaceCard(
      key: ValueKey('guideline-proposal-${p.id}'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(proposalKindLabel(p), style: TextStyle(fontSize: 13, color: Tokens.label2)),
          const SizedBox(height: 4),
          Text(p.title, style: Tokens.title(size: 18)),
          Text(proposalByline(p, widget.proposer), style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
          if (current != null && current.title != p.title)
            Text('Renames “${current.title}”', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
          if (p.reason.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(p.reason, style: const TextStyle(fontSize: 14)),
          ],
          if (p.evidence.isNotEmpty) ...[
            const SizedBox(height: 8),
            Wrap(spacing: 6, runSpacing: 4, children: [
              for (final e in p.evidence)
                Chip(
                  visualDensity: VisualDensity.compact,
                  avatar: Icon(e.kind == 'task' ? Icons.task_alt : Icons.lightbulb_outline, size: 15),
                  label: Text(e.label, overflow: TextOverflow.ellipsis),
                ),
            ]),
          ],
          if (stale && current != null)
            Padding(
              key: ValueKey('guideline-proposal-stale-${p.id}'),
              padding: const EdgeInsets.only(top: 10),
              child: Text(staleLine(p, current, widget.proposer), style: TextStyle(color: Tokens.negative, fontSize: 12.5)),
            ),
          const SizedBox(height: 10),
          DiffLines(key: ValueKey('guideline-proposal-diff-${p.id}'), lines: lineDiff(item.baseText ?? '', p.text)),
          const SizedBox(height: 8),
          Text(
            [counterLine(count), ?limitNote(count)].join(' · '),
            key: ValueKey('guideline-proposal-count-${p.id}'),
            style: TextStyle(fontSize: 12.5, color: over ? Tokens.negative : limitLevel(count, guidelineMaxChars) == LimitLevel.close ? Tokens.warning : Tokens.label2),
          ),
          if (largest != null)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(
                'Largest block after this: ${contextName(largest.context, employee: names.employee, project: names.project)} · ${counterLine(largest.chars, max: blockMaxChars)}',
                key: ValueKey('guideline-proposal-block-${p.id}'),
                style: TextStyle(fontSize: 12.5, color: switch (limitLevel(largest.chars, blockMaxChars)) { LimitLevel.ok => Tokens.label2, LimitLevel.close => Tokens.warning, LimitLevel.over => Tokens.negative }),
              ),
            ),
          if (tooLarge != null && !stale)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(tooLarge, key: ValueKey('guideline-proposal-too-large-${p.id}'), style: TextStyle(fontSize: 12.5, color: Tokens.negative)),
            ),
          if (item.isNew)
            GroupRow(
              key: ValueKey('guideline-proposal-links-${p.id}'),
              padding: const EdgeInsets.symmetric(vertical: 8),
              title: 'Applies to: ${_links.isEmpty ? 'nobody yet' : linksLine(_targets, employee: names.employee, project: names.project, shown: 3)}',
              onTap: widget.canDecide && !widget.deciding ? _chooseLinks : null,
              chevron: widget.canDecide,
            ),
          if (widget.canDecide)
            // A Wrap, so the three buttons fit a narrow phone.
            Wrap(
              alignment: WrapAlignment.end,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                TextButton(key: ValueKey('guideline-reject-${p.id}'), onPressed: widget.deciding ? null : _reject, child: const Text('Reject')),
                if (!stale) ...[
                  const SizedBox(width: 4),
                  TextButton(key: ValueKey('guideline-edit-${p.id}'), onPressed: widget.deciding ? null : _edit, child: const Text('Edit')),
                  const SizedBox(width: 4),
                  FilledButton(
                    key: ValueKey('guideline-approve-${p.id}'),
                    onPressed: widget.deciding || over || tooLarge != null ? null : () => widget.onDecide(true, links: item.isNew ? _targets : null),
                    child: const Text('Approve'),
                  ),
                ],
              ],
            )
          else
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text('A board member decides.', style: TextStyle(fontSize: 12.5, color: Tokens.label2)),
            ),
        ],
      ),
    );
  }
}

/// The card in the Inbox, wired to the [InboxBloc].
class InboxGuidelineProposal extends StatelessWidget {
  const InboxGuidelineProposal({super.key, required this.item});

  final GuidelineProposalItem item;

  @override
  Widget build(BuildContext context) {
    final names = Names.of(context);
    final proposer = names.employee(item.proposal.proposedBy);
    final deciding = context.select<InboxBloc, bool>((b) => b.state.deciding.contains('guideline-proposal:${item.id}'));
    final link = context.select<ConnectionBloc, LinkState>((b) => b.state);
    return GuidelineProposalCard(
      item: item,
      proposer: proposer,
      canDecide: canDecideGuidelines(link),
      deciding: deciding,
      onDecide: (approve, {note, title, text, links}) => context.read<InboxBloc>().add(GuidelineProposalDecided(
            item,
            approve: approve,
            note: note,
            title: title,
            text: text,
            links: links,
            proposer: proposer,
            employee: names.employee,
            project: names.project,
          )),
    );
  }
}

/// One proposal on its own page (from a guideline's bar), as the Inbox shows it.
Future<void> openGuidelineProposal(BuildContext context, String proposalId) => Navigator.of(context).push<void>(MaterialPageRoute(
      builder: (_) => Scaffold(
        appBar: GlassAppBar(title: const Text('Proposal')),
        body: BlocConsumer<InboxBloc, InboxState>(
          listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
          listener: (context, s) {
            toast(context, s.notice!);
            context.read<InboxBloc>().add(const InboxNoticeShown());
          },
          builder: (context, s) {
            final item = s.guidelineProposals.where((g) => g.id == proposalId).firstOrNull;
            return item == null
                ? const EmptyState('This proposal was decided', detail: 'It is no longer waiting for a board member.')
                : ListView(padding: const EdgeInsets.all(16), children: [InboxGuidelineProposal(item: item)]);
          },
        ),
      ),
    ));
