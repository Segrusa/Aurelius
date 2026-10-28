import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart' show InfoIcon;
import '../../data/views.dart';
import '../chat/chat_view.dart' show markdownStyle;
import '../markdown/links.dart';
import '../markdown/markdown_page.dart' show markdownLinkTap;
import '../connection/connection_bloc.dart';
import 'inbox_bloc.dart';

/// Open Claude's plan in a bottom sheet: the plan (markdown, scrollable), a
/// revision field with "Pas aan" (feedback required) that rides above the
/// keyboard, "Goedkeuren + Auto" (the worker goes on in auto mode),
/// "Goedkeuren" (edits go through, it asks for the rest) and reject.
Future<void> showPlanReview(BuildContext context, ApprovalView approval) {
  final inbox = context.read<InboxBloc>();
  final connection = context.read<ConnectionBloc>();
  return showModalBottomSheet<void>(
    context: context,
    useRootNavigator: true,
    isScrollControlled: true,
    useSafeArea: true,
    showDragHandle: true,
    backgroundColor: Tokens.panel,
    builder: (_) => MultiBlocProvider(
      providers: [BlocProvider.value(value: inbox), BlocProvider.value(value: connection)],
      child: PlanReview(approval: approval),
    ),
  );
}

/// The plan as it was when the sheet opened; what happens to it afterwards
/// (decided here, decided on another device, expired) comes from the inbox.
class PlanReview extends StatefulWidget {
  const PlanReview({super.key, required this.approval});

  final ApprovalView approval;

  @override
  State<PlanReview> createState() => _PlanReviewState();
}

/// One inline comment on the plan: the selected fragment and what the user says about it.
typedef PlanComment = ({String quote, String text});

/// The feedback for the agent: per comment a blockquote of the fragment followed by the
/// comment, then the free text. Empty when there is nothing.
String composePlanFeedback(List<PlanComment> comments, String free) {
  const nl = '\n';
  final parts = <String>[
    for (final c in comments)
      '${c.quote.trim().split(nl).map((l) => l.trim().isEmpty ? '>' : '> ${l.trimRight()}').join(nl)}$nl$nl${c.text.trim()}',
    if (free.trim().isNotEmpty) free.trim(),
  ];
  return parts.join('$nl$nl');
}

class _PlanReviewState extends State<PlanReview> {
  final _feedback = TextEditingController();
  final _comments = <PlanComment>[];
  String _selected = '';

  @override
  void initState() {
    super.initState();
    _feedback.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _feedback.dispose();
    super.dispose();
  }

  void _decide(PlanDecision decision) {
    HapticFeedback.mediumImpact();
    // Feedback goes with changes and with a rejection; an approval carries it only when there are comments.
    final text = decision.approves && _comments.isEmpty ? '' : composePlanFeedback(_comments, _feedback.text);
    context.read<InboxBloc>().add(PlanDecided(widget.approval, decision, feedback: text.isEmpty ? null : text));
  }

  /// Ask for the comment on the selected fragment and keep it.
  Future<void> _addComment(String quote) async {
    final field = TextEditingController();
    final text = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Comment'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(quote.length > 160 ? '${quote.substring(0, 160)}…' : quote, style: TextStyle(color: Tokens.muted)),
            const SizedBox(height: 12),
            TextField(key: const Key('plan-comment-field'), controller: field, autofocus: true, minLines: 1, maxLines: 4, textCapitalization: TextCapitalization.sentences, decoration: const InputDecoration(labelText: 'Your comment')),
          ],
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('Cancel')),
          TextButton(key: const Key('plan-comment-save'), onPressed: () => Navigator.pop(ctx, field.text.trim()), child: const Text('Add')),
        ],
      ),
    );
    field.dispose();
    if (text == null || text.isEmpty || !mounted) return;
    setState(() => _comments.add((quote: quote, text: text)));
  }

  /// (text, color) of the status line once the plan is no longer open, or `null` while it is.
  (String, Color)? _status(InboxState state, bool expired) {
    final outcome = state.planOutcomes[widget.approval.id];
    if (outcome != null) {
      return switch (outcome) {
        PlanOutcome.approved => ('Approved. The worker goes on in auto mode.', Tokens.accentText),
        PlanOutcome.approvedEdits => ('Approved. Edits go through; the worker asks for the rest.', Tokens.accentText),
        PlanOutcome.revisionRequested => ('Changes requested. The worker revises the plan.', Tokens.warning),
        PlanOutcome.rejected => ('Rejected. The worker stops.', Tokens.negative),
        PlanOutcome.expired => ('Expired. Nothing was approved.', Tokens.warning),
        PlanOutcome.handledElsewhere => ('Already handled on another device.', Tokens.text2),
      };
    }
    // Gone from the inbox without a decision here: handled elsewhere.
    if (!state.loading && state.approval(widget.approval.id) == null) return ('Already handled on another device.', Tokens.text2);
    if (expired) return ('Expired. Expiry never counts as permission.', Tokens.warning);
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final state = context.watch<InboxBloc>().state;
    final online = context.watch<ConnectionBloc>().state.isOnline;
    final approval = widget.approval;
    final deciding = state.deciding.contains(approval.id);
    final minutes = ((approval.expiresAtMs - DateTime.now().millisecondsSinceEpoch) / 60000).ceil();
    final status = _status(state, minutes <= 0);
    final open = status == null;
    final canAct = open && online && !deciding;
    final feedback = _feedback.text.trim();
    final hasFeedback = feedback.isNotEmpty || _comments.isNotEmpty;
    final plan = approval.planMarkdown;
    // The whole sheet rides above the keyboard, so the field and buttons stay in view.
    final keyboard = MediaQuery.viewInsetsOf(context).bottom;
    return AnimatedPadding(
      duration: const Duration(milliseconds: 150),
      padding: EdgeInsets.only(bottom: keyboard),
      child: Column(
        mainAxisSize: MainAxisSize.max,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 0, 20, 8),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(approval.title, style: TextStyle(color: Tokens.text2)),
                      const SizedBox(height: 2),
                      Text('Plan', style: Tokens.title(size: 24)),
                    ],
                  ),
                ),
                if (open) Text(minutes <= 5 ? '$minutes min left' : '$minutes min', style: Tokens.monoSmall(color: minutes <= 5 ? Tokens.warning : Tokens.muted)),
              ],
            ),
          ),
          if (status != null)
            Container(
              key: const Key('plan-status'),
              margin: const EdgeInsets.fromLTRB(16, 0, 16, 8),
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
              decoration: BoxDecoration(color: Tokens.surface, borderRadius: BorderRadius.circular(Tokens.radius), border: Border.all(color: status.$2.withValues(alpha: 0.6))),
              child: Text(status.$1, style: TextStyle(color: status.$2, fontWeight: FontWeight.w600)),
            ),
          Expanded(
            child: plan.trim().isEmpty
                ? Center(child: Text('This plan has no text.', style: TextStyle(color: Tokens.muted)))
                : SelectionArea(
                    onSelectionChanged: (content) => _selected = content?.plainText ?? '',
                    contextMenuBuilder: (context, selection) => AdaptiveTextSelectionToolbar.buttonItems(
                      anchors: selection.contextMenuAnchors,
                      buttonItems: [
                        ...selection.contextMenuButtonItems,
                        if (open)
                          ContextMenuButtonItem(
                            label: 'Comment',
                            onPressed: () {
                              final quote = _selected.trim();
                              selection.hideToolbar();
                              if (quote.isNotEmpty) _addComment(quote);
                            },
                          ),
                      ],
                    ),
                    child: LinkGestures(
                      builder: (context, copied) => Markdown(
                        key: const Key('plan-text'),
                        data: plan,
                        selectable: false,
                        styleSheet: markdownStyle(context),
                        padding: const EdgeInsets.fromLTRB(20, 4, 20, 16),
                        onTapLink: markdownLinkTap(context, null, copied: copied),
                      ),
                    ),
                  ),
          ),
          if (_comments.isNotEmpty)
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 160),
              child: ListView(
                key: const Key('plan-comments'),
                shrinkWrap: true,
                padding: const EdgeInsets.symmetric(horizontal: 16),
                children: [
                  for (var i = 0; i < _comments.length; i++)
                    ListTile(
                      dense: true,
                      contentPadding: EdgeInsets.zero,
                      title: Text('> ${_comments[i].quote}', maxLines: 2, overflow: TextOverflow.ellipsis, style: TextStyle(color: Tokens.muted, fontSize: 12)),
                      subtitle: Text(_comments[i].text),
                      trailing: IconButton(key: Key('plan-comment-remove-$i'), icon: const Icon(Icons.close, size: 18), tooltip: 'Remove comment', onPressed: open ? () => setState(() => _comments.removeAt(i)) : null),
                    ),
                ],
              ),
            ),
          // Quick revision right under the plan: type what to change, "Pas aan".
          if (open)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.center,
                children: [
                  Expanded(
                    child: TextField(
                      key: const Key('plan-feedback'),
                      controller: _feedback,
                      enabled: canAct,
                      minLines: 1,
                      maxLines: 4,
                      textCapitalization: TextCapitalization.sentences,
                      decoration: InputDecoration(
                        isDense: true,
                        fillColor: Tokens.cell2,
                        labelText: 'Wat moet er anders?',
                        suffixIcon: const InfoIcon('Gaat mee met Pas aan (verplicht) of Afwijzen (optioneel), niet met goedkeuren.', label: 'Feedback'),
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  _PlanButton(
                    key: const Key('plan-changes'),
                    icon: Icons.edit_note,
                    label: 'Pas aan',
                    tooltip: 'Vraag Claude het plan aan te passen',
                    onPressed: canAct && hasFeedback ? () => _decide(PlanDecision.revise) : null,
                  ),
                ],
              ),
            ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
            child: Row(
              children: [
                Expanded(
                  child: _PlanButton(
                    key: const Key('plan-approve'),
                    icon: Icons.check,
                    label: 'Goedkeuren + Auto',
                    tooltip: 'Goedkeuren; het werk gaat verder in Auto',
                    busy: deciding,
                    onPressed: canAct && minutes > 0 ? () => _decide(PlanDecision.approve) : null,
                    primary: true,
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: _PlanButton(
                    key: const Key('plan-approve-edits'),
                    icon: Icons.edit_outlined,
                    label: 'Goedkeuren',
                    tooltip: 'Goedkeuren; bewerkingen gaan door, de rest vraagt eerst',
                    onPressed: canAct && minutes > 0 ? () => _decide(PlanDecision.approveEdits) : null,
                  ),
                ),
              ],
            ),
          ),
          Padding(
            padding: EdgeInsets.fromLTRB(16, 0, 16, 16 + MediaQuery.paddingOf(context).bottom),
            child: _PlanButton(
              key: const Key('plan-reject'),
              icon: Icons.close,
              label: 'Afwijzen',
              onPressed: canAct ? () => _decide(PlanDecision.reject) : null,
              danger: true,
            ),
          ),
        ],
      ),
    );
  }
}

/// One of the four floating buttons under the plan.
class _PlanButton extends StatelessWidget {
  const _PlanButton({super.key, required this.icon, required this.label, required this.onPressed, this.tooltip, this.busy = false, this.primary = false, this.danger = false});

  final IconData icon;
  final String label;
  final String? tooltip;
  final VoidCallback? onPressed;
  final bool busy;
  final bool primary;
  final bool danger;

  @override
  Widget build(BuildContext context) {
    final enabled = onPressed != null;
    final background = !enabled
        ? Tokens.fill
        : primary
            ? Tokens.accent
            : Tokens.cell2;
    final foreground = !enabled
        ? Tokens.label3
        : primary
            ? Tokens.accentInk
            : danger
                ? Tokens.red
                : Tokens.label;
    return Tooltip(
      message: tooltip ?? label,
      child: FloatingActionButton.extended(
        heroTag: null,
        onPressed: onPressed,
        elevation: 0,
        disabledElevation: 0,
        highlightElevation: 0,
        focusElevation: 0,
        hoverElevation: 0,
        shape: const StadiumBorder(),
        extendedTextStyle: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
        backgroundColor: background,
        foregroundColor: foreground,
        icon: busy ? SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2, color: foreground)) : Icon(icon, size: 20),
        label: Text(label),
      ),
    );
  }
}
