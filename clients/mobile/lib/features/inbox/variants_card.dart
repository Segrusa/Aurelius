import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/widgets.dart';
import '../../data/views.dart';
import 'diff_view.dart';

/// A task's variants side by side once they are ready (feature
/// `task_variants`): per variant its summary, changes, diff, tokens and the
/// judge's note, with Choose per variant or "Let the judge choose".
/// [variants] null: still loading; the card asks for it through [onLoad].
class VariantsCard extends StatefulWidget {
  const VariantsCard({
    super.key,
    required this.task,
    required this.variants,
    required this.onLoad,
    required this.onChoose,
    this.subtitle,
    this.onOpen,
    this.choosing = false,
  });

  final TaskView task;
  final TaskVariantsView? variants;
  final VoidCallback onLoad;

  /// The variant to apply; null takes the judge's pick.
  final void Function(int? variant) onChoose;

  /// Project and employee, under the title.
  final String? subtitle;
  final VoidCallback? onOpen;

  /// A choice is on its way.
  final bool choosing;

  @override
  State<VariantsCard> createState() => _VariantsCardState();
}

class _VariantsCardState extends State<VariantsCard> {
  @override
  void initState() {
    super.initState();
    if (widget.variants == null) WidgetsBinding.instance.addPostFrameCallback((_) => widget.onLoad());
  }

  Future<void> _choose(int? n) async {
    final what = n == null ? "the judge's pick" : 'variant $n';
    if (await confirm(context, title: 'Apply $what?', body: 'Its changes go into the work folder; the other variants are dropped.', action: 'Apply')) {
      widget.onChoose(n);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = widget.task;
    final v = widget.variants;
    final ready = v != null && v.phase == VariantPhase.ready;
    return SurfaceCard(
      key: ValueKey('variants-${t.id}'),
      onTap: widget.onOpen,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(v == null ? 'Variants ready to compare' : 'Variants ready to compare · ${v.count}', style: TextStyle(fontSize: 13, color: Tokens.label2)),
          const SizedBox(height: 6),
          Text(t.title, style: TextStyle(fontSize: 17, fontWeight: FontWeight.w600, color: Tokens.label)),
          if (widget.subtitle?.isNotEmpty == true) Text(widget.subtitle!, style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
          if (v == null)
            const Padding(padding: EdgeInsets.symmetric(vertical: 16), child: Center(child: CircularProgressIndicator()))
          else ...[
            if (v.judgePick != null || (v.compose ?? '').isNotEmpty)
              Container(
                key: ValueKey('variants-judge-${t.id}'),
                margin: const EdgeInsets.only(top: 10),
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(color: Tokens.accentTint, borderRadius: BorderRadius.circular(Tokens.radius)),
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text(v.judgePick == null ? 'Judge' : 'Judge advises variant ${v.judgePick}', style: TextStyle(fontWeight: FontWeight.w600, color: Tokens.accent)),
                  if ((v.compose ?? '').isNotEmpty) Padding(padding: const EdgeInsets.only(top: 4), child: Text(v.compose!, style: const TextStyle(fontSize: 13.5, height: 1.35))),
                ]),
              ),
            if (v.error != null) Padding(padding: const EdgeInsets.only(top: 8), child: Text(v.error!, style: TextStyle(color: Tokens.warning, fontSize: 12.5))),
            for (final trial in v.variants)
              _Variant(
                trial: trial,
                pick: trial.n == v.judgePick,
                onChoose: ready && trial.choosable && !widget.choosing ? () => _choose(trial.n) : null,
              ),
            const SizedBox(height: 12),
            if (!ready)
              Text(variantPhaseLabel(v.phase), style: TextStyle(color: Tokens.label2, fontSize: 13))
            else if (v.judgePick != null)
              SizedBox(
                width: double.infinity,
                child: FilledButton(
                  key: ValueKey('variants-judge-choose-${t.id}'),
                  onPressed: widget.choosing ? null : () => _choose(null),
                  child: Text('Let the judge choose (variant ${v.judgePick})'),
                ),
              ),
          ],
        ],
      ),
    );
  }
}

class _Variant extends StatelessWidget {
  const _Variant({required this.trial, required this.pick, required this.onChoose});

  final TaskVariantView trial;
  final bool pick;
  final VoidCallback? onChoose;

  @override
  Widget build(BuildContext context) {
    final t = trial;
    final failed = t.status == VariantStatus.failed || t.status == VariantStatus.cancelled;
    final facts = [
      if (t.status != VariantStatus.done) t.status.wire,
      t.changeLine,
      '${Fmt.tokens(t.tokens)} tokens',
      if (t.errors > 0) '${t.errors} error${t.errors == 1 ? '' : 's'}',
      if ((t.tests ?? '').isNotEmpty) 'tests: ${t.tests}',
    ].join(' · ');
    return Container(
      key: ValueKey('variant-${t.n}'),
      margin: const EdgeInsets.only(top: 10),
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        border: Border.all(color: pick ? Tokens.accent : Tokens.sep),
        borderRadius: BorderRadius.circular(Tokens.radius),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(children: [
            Expanded(
              child: Text(
                [
                  'Variant ${t.n}',
                  if (t.hint.isNotEmpty) t.hint,
                ].join(' · '),
                style: TextStyle(fontWeight: FontWeight.w600, color: failed ? Tokens.muted : Tokens.label),
              ),
            ),
            if (t.rank != null)
              Text(t.rank == 1 ? 'Best' : '#${t.rank}', style: Tokens.monoSmall(color: t.rank == 1 ? Tokens.accent : Tokens.label2)),
          ]),
          const SizedBox(height: 4),
          Text(facts, style: Tokens.monoSmall()),
          if (t.judgeNote?.isNotEmpty == true) Padding(padding: const EdgeInsets.only(top: 6), child: Text('Judge: ${t.judgeNote}', style: TextStyle(fontSize: 13, color: Tokens.text2, fontStyle: FontStyle.italic))),
          if (t.report.isNotEmpty) Padding(padding: const EdgeInsets.only(top: 6), child: Text(t.report, maxLines: 5, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 13.5, height: 1.35))),
          if (t.diff.isNotEmpty)
            Theme(
              data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
              child: ExpansionTile(
                key: ValueKey('variant-diff-${t.n}'),
                tilePadding: EdgeInsets.zero,
                childrenPadding: const EdgeInsets.only(bottom: 6),
                title: Text('Diff', style: TextStyle(fontSize: 14, color: Tokens.accent)),
                children: [
                  DiffLines.unified(t.diff, collapsedLines: 40),
                  if (t.diffTruncated) Padding(padding: const EdgeInsets.only(top: 4), child: Text('Diff cut short.', style: TextStyle(color: Tokens.muted, fontSize: 12))),
                ],
              ),
            ),
          if (onChoose != null)
            Align(
              alignment: Alignment.centerRight,
              child: OutlinedButton(key: ValueKey('variant-choose-${t.n}'), onPressed: onChoose, child: Text('Choose variant ${t.n}')),
            ),
        ],
      ),
    );
  }
}
