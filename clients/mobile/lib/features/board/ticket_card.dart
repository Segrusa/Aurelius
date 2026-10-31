import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import 'ticket_actions.dart';

/// What a card needs from the board around it.
class CardContext {
  const CardContext({required this.columns, required this.column, this.needsOwner = false, this.ownerUnknown = false, this.showProject = true, this.online = true, this.wide = false});

  /// Every column in board order (Move to…).
  final List<BoardColumnView> columns;

  /// The card's own column with its cards (Move up / down).
  final BoardColumnCards column;

  /// In Todo: nobody chooses its owner (B5).
  final bool needsOwner;

  /// Whether the dispatcher's give-up could not be read (M-m3).
  final bool ownerUnknown;

  /// Hidden when the project filter is on that project (§5.1).
  final bool showProject;

  /// Offline the actions are off ("Offline · reconnecting", §3.3).
  final bool online;

  /// Tablet: long press drags, so the menu sits behind ⋯.
  final bool wide;
}

/// The card's main action, by column (§4.2, §5.1): Start, Choose owner, Review, Start again.
enum CardAction { start, chooseOwner, reviewPlan, review, startAgain }

CardAction? cardAction(TicketState state) => switch (state) {
      TicketState.backlog => CardAction.start,
      TicketState.needsOwner || TicketState.ownerUnknown => CardAction.chooseOwner,
      TicketState.planReview => CardAction.reviewPlan,
      TicketState.review => CardAction.review,
      TicketState.failed => CardAction.startAgain,
      _ => null,
    };

String cardActionLabel(CardAction a) => switch (a) {
      CardAction.start => 'Start',
      CardAction.chooseOwner => 'Choose owner',
      CardAction.reviewPlan || CardAction.review => 'Review',
      CardAction.startAgain => 'Start again',
    };

/// Do the card's main action.
Future<void> runCardAction(BuildContext context, TicketView ticket, CardAction action) async {
  switch (action) {
    case CardAction.start || CardAction.startAgain:
      await startTicket(context, ticket);
    case CardAction.chooseOwner:
      await chooseOwner(context, ticket);
    case CardAction.reviewPlan:
      context.push('/ticket/${ticket.id}/plan');
    case CardAction.review:
      context.push('/ticket/${ticket.id}');
  }
}

/// A ticket on the Board (§5, MU1-07): number, project, priority, title,
/// labels, where it stands, owner, deadline and creator, with the column's
/// main action. Swipe right = that action, left = Move to…; long press = the
/// menu (§4.2). Screen readers get the same actions (§15).
class TicketCard extends StatelessWidget {
  const TicketCard({super.key, required this.ticket, required this.board});

  final TicketView ticket;
  final CardContext board;

  @override
  Widget build(BuildContext context) {
    final company = context.watch<CompanyBloc>().state;
    final root = company.tasks.where((t) => t.id == ticket.rootTaskId).firstOrNull;
    final state = ticketState(ticket, root: root, needsOwner: board.needsOwner, ownerUnknown: board.ownerUnknown);
    final delegations = company.tasks.where((t) => t.ticketId == ticket.id && t.parentTaskId != null && t.isOpen).length;
    final owner = ticket.ownerName ?? employeeName(company, ticket.ownerEmployeeId);
    final dispatcher = dispatcherName(company, ticket.projectId);
    final action = cardAction(state);
    final deadline = deadlineOf(ticket);
    final done = state == TicketState.done;
    final line = state.line(owner: owner, dispatcher: dispatcher);

    final summary = [
      'Ticket ${ticket.number}, ${ticket.title}',
      if (ticket.priority != TicketPriority.normal) 'priority ${ticket.priority.label}',
      owner == null ? 'no owner' : 'owner $owner',
      ?line,
      if (state.needsYou) 'needs you',
      if (deadline != null) deadline.text,
      'in ${board.column.column.name}',
    ].join(', ');

    final body = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(children: [
          Text('#${ticket.number}', style: Tokens.monoSmall()),
          if (board.showProject) ...[
            const SizedBox(width: 8),
            Flexible(child: Text(ticket.projectName, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 13, color: Tokens.label2))),
          ] else
            const Spacer(),
          if (board.showProject) const Spacer(),
          if (ticket.priority.mark case final mark?)
            Text(mark, style: TextStyle(fontSize: 13, fontWeight: FontWeight.w600, color: priorityColor(ticket.priority))),
          if (board.wide)
            SizedBox(
              width: 32,
              height: 28,
              child: IconButton(
                key: ValueKey('card-menu-${ticket.id}'),
                padding: EdgeInsets.zero,
                iconSize: 18,
                tooltip: 'Ticket actions',
                icon: const Icon(CupertinoIcons.ellipsis),
                onPressed: () => showCardMenu(context, ticket, board, state),
              ),
            ),
        ]),
        const SizedBox(height: 4),
        Text(ticket.title, maxLines: MediaQuery.textScalerOf(context).scale(1) >= 1.3 ? null : 3, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 15, fontWeight: FontWeight.w600, color: Tokens.label)),
        if (ticket.labels.isNotEmpty) ...[
          const SizedBox(height: 6),
          Wrap(spacing: 6, runSpacing: 4, children: [
            for (final l in ticket.labels.take(2)) _Pill(l),
            if (ticket.labels.length > 2) _Pill('+${ticket.labels.length - 2}'),
          ]),
        ],
        if (line != null && !done || delegations > 0) ...[
          const SizedBox(height: 6),
          Row(children: [
            if (line != null && !done) ...[
              _StateMark(state),
              const SizedBox(width: 6),
              Flexible(child: Text(line, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 13, color: state.needsYou ? Tokens.accentText : Tokens.label2, fontWeight: state.needsYou ? FontWeight.w600 : null))),
            ],
            const Spacer(),
            if (delegations > 0) Text('⑂ $delegations task${delegations == 1 ? '' : 's'}', style: TextStyle(fontSize: 13, color: Tokens.label2)),
          ]),
        ],
        const SizedBox(height: 6),
        Wrap(
          spacing: 8,
          runSpacing: 2,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            if (done) Text('✓ Done${ticket.doneAtMs == null ? '' : ' ${DateFormat.MMMd().format(DateTime.fromMillisecondsSinceEpoch(ticket.doneAtMs!))}'}', style: TextStyle(fontSize: 13, color: Tokens.label2)),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 200),
              child: Row(mainAxisSize: MainAxisSize.min, children: [
                if (owner != null) ...[Monogram(owner.characters.first.toUpperCase(), size: 18), const SizedBox(width: 4)],
                Flexible(child: Text(owner ?? 'No owner', maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 13, color: Tokens.label2))),
              ]),
            ),
            if (deadline != null)
              Text(deadline.text, style: TextStyle(fontSize: 13, color: deadline.late ? Tokens.negative : deadline.soon ? Tokens.warning : Tokens.label2)),
            Text('by ${ticket.createdByUsername}', style: TextStyle(fontSize: 13, color: Tokens.label2)),
          ],
        ),
        if (action != null && !done)
          Align(
            alignment: Alignment.centerRight,
            child: Padding(
              padding: const EdgeInsets.only(top: 6),
              child: FilledButton.tonal(
                key: ValueKey('card-action-${ticket.id}'),
                style: FilledButton.styleFrom(minimumSize: const Size(44, 44), shape: const StadiumBorder(), padding: const EdgeInsets.symmetric(horizontal: 16)),
                onPressed: board.online ? () => runCardAction(context, ticket, action) : null,
                child: Text(action == CardAction.start ? '▸ Start' : cardActionLabel(action)),
              ),
            ),
          ),
      ],
    );

    final card = Opacity(
      opacity: done ? 0.75 : 1,
      child: Material(
        color: Tokens.cell,
        clipBehavior: Clip.antiAlias,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(18),
          side: state.needsYou ? BorderSide(color: Tokens.accent.withValues(alpha: 0.45)) : BorderSide.none,
        ),
        child: InkWell(
          onTap: () => context.push('/ticket/${ticket.id}'),
          onLongPress: board.wide ? null : () => showCardMenu(context, ticket, board, state),
          child: Container(
            decoration: state.needsYou ? BoxDecoration(border: Border(left: BorderSide(color: Tokens.accent, width: 3))) : null,
            padding: const EdgeInsets.fromLTRB(14, 12, 12, 12),
            child: body,
          ),
        ),
      ),
    );

    final semantic = Semantics(
      container: true,
      label: summary,
      customSemanticsActions: {
        const CustomSemanticsAction(label: 'Open'): () => context.push('/ticket/${ticket.id}'),
        if (action != null && board.online) CustomSemanticsAction(label: cardActionLabel(action)): () => runCardAction(context, ticket, action),
        if (!done && board.online) const CustomSemanticsAction(label: 'Move to…'): () => showMoveSheet(context, ticket, board.columns),
      },
      child: card,
    );

    // Swipes as in Mail (§4.2): right = the main action, left = Move to….
    if (done || !board.online || board.wide) return semantic;
    return Dismissible(
      key: ValueKey('swipe-${ticket.id}'),
      direction: action == null ? DismissDirection.endToStart : DismissDirection.horizontal,
      confirmDismiss: (direction) async {
        HapticFeedback.selectionClick();
        if (direction == DismissDirection.startToEnd && action != null) {
          await runCardAction(context, ticket, action);
        } else if (direction == DismissDirection.endToStart && context.mounted) {
          await showMoveSheet(context, ticket, board.columns);
        }
        // The card never leaves by itself; the board follows the server.
        return false;
      },
      background: _SwipeFace(color: Tokens.accent, ink: Tokens.accentInk, icon: CupertinoIcons.play_fill, label: action == null ? '' : cardActionLabel(action), alignment: Alignment.centerLeft),
      secondaryBackground: _SwipeFace(color: Tokens.gray, ink: Colors.white, icon: CupertinoIcons.arrow_right_arrow_left, label: 'Move to…', alignment: Alignment.centerRight),
      child: semantic,
    );
  }
}

Color priorityColor(TicketPriority p) => switch (p) {
      TicketPriority.urgent => Tokens.negative,
      TicketPriority.high => Tokens.warning,
      _ => Tokens.label2,
    };

/// Long press (phone) or ⋯ (tablet): Open · Start · Start with… · Move to… ·
/// Move up / down · Copy · Back to Backlog · Archive (§4.2).
Future<void> showCardMenu(BuildContext context, TicketView ticket, CardContext board, TicketState state) async {
  final index = board.column.tickets.indexWhere((t) => t.id == ticket.id);
  final canOrder = ticket.column != ColumnKind.done && board.online;
  final items = <(String, IconData, Future<void> Function())>[
    ('Open', CupertinoIcons.arrow_up_right_square, () async => context.push('/ticket/${ticket.id}')),
    if (state == TicketState.backlog && board.online) ...[
      ('Start', CupertinoIcons.play_fill, () => startTicket(context, ticket)),
      ('Start with…', CupertinoIcons.person_crop_circle_badge_plus, () => startWith(context, ticket)),
    ],
    if (state == TicketState.choosing || state == TicketState.needsOwner || state == TicketState.ownerUnknown) ('Choose employee', CupertinoIcons.person_crop_circle, () => chooseOwner(context, ticket)),
    if (ticket.column != ColumnKind.done && board.online) ('Move to…', CupertinoIcons.arrow_right_arrow_left, () => showMoveSheet(context, ticket, board.columns)),
    if (canOrder && index > 0) ('Move up', CupertinoIcons.arrow_up, () => nudgeTicket(context, ticket, board.column, -1)),
    if (canOrder && index >= 0 && index < board.column.tickets.length - 1) ('Move down', CupertinoIcons.arrow_down, () => nudgeTicket(context, ticket, board.column, 1)),
    ('Copy #${ticket.number}', CupertinoIcons.doc_on_doc, () async {
      await Clipboard.setData(ClipboardData(text: '#${ticket.number} ${ticket.title}'));
      if (context.mounted) toast(context, 'Copied #${ticket.number}.');
    }),
    if (ticket.column != ColumnKind.backlog && ticket.column != ColumnKind.done && board.online)
      (ticket.column == ColumnKind.planReview ? 'Afwijzen' : 'Back to Backlog', CupertinoIcons.arrow_uturn_left, () => backToBacklog(context, ticket)),
    if ((ticket.column == ColumnKind.backlog || ticket.column == ColumnKind.done) && board.online)
      ('Archive', CupertinoIcons.archivebox, () async {
        await ticketWrite(context, ticketArchive(ticketId: ticket.id), expectedVersion: ticket.version, retry: false, done: '#${ticket.number} is archived.');
      }),
  ];
  final picked = await showModalBottomSheet<int>(
    context: context,
    useRootNavigator: true,
    showDragHandle: true,
    builder: (sheet) => SafeArea(
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        Padding(padding: const EdgeInsets.fromLTRB(20, 0, 20, 8), child: Text('#${ticket.number} ${ticket.title}', maxLines: 2, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 17, fontWeight: FontWeight.w600))),
        for (final (i, item) in items.indexed)
          ListTile(key: ValueKey('card-menu-item-${item.$1}'), leading: Icon(item.$2), title: Text(item.$1), onTap: () => Navigator.pop(sheet, i)),
      ]),
    ),
  );
  if (picked != null && context.mounted) await items[picked].$3();
}

/// "Start with…" (MU1-12): choose the employee, then Start without the dispatcher.
Future<void> startWith(BuildContext context, TicketView ticket) async {
  final company = context.read<CompanyBloc>().state;
  final active = company.employees.where((e) => e.status == LifecycleStatus.active).toList()..sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
  final id = await showModalBottomSheet<String>(
    context: context,
    useRootNavigator: true,
    showDragHandle: true,
    isScrollControlled: true,
    builder: (sheet) => SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
        child: ListView(shrinkWrap: true, children: [
          Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow('Start #${ticket.number} with…')),
          for (final e in active) ListTile(key: ValueKey('start-with-${e.id}'), title: Text(e.name), subtitle: Text(e.role), onTap: () => Navigator.pop(sheet, e.id)),
        ]),
      ),
    ),
  );
  if (id != null && context.mounted) await startTicket(context, ticket, employeeId: id);
}

class _StateMark extends StatelessWidget {
  const _StateMark(this.state);

  final TicketState state;

  @override
  Widget build(BuildContext context) => switch (state) {
        TicketState.working => const StatusDot(pulse: true),
        TicketState.choosing => Icon(CupertinoIcons.circle_lefthalf_fill, size: 12, color: Tokens.label2),
        TicketState.failed => Text('✕', style: TextStyle(color: Tokens.negative, fontWeight: FontWeight.w700)),
        _ when state.needsYou => Text('◆', style: TextStyle(color: Tokens.accent, fontSize: 12)),
        _ => StatusDot(color: Tokens.label3),
      };
}

class _Pill extends StatelessWidget {
  const _Pill(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
        decoration: BoxDecoration(color: Tokens.cell2, borderRadius: BorderRadius.circular(999)),
        child: Text(text, style: TextStyle(fontSize: 12, color: Tokens.label2)),
      );
}

class _SwipeFace extends StatelessWidget {
  const _SwipeFace({required this.color, required this.ink, required this.icon, required this.label, required this.alignment});

  final Color color;
  final Color ink;
  final IconData icon;
  final String label;
  final Alignment alignment;

  @override
  Widget build(BuildContext context) => Container(
        decoration: BoxDecoration(color: color, borderRadius: BorderRadius.circular(18)),
        padding: const EdgeInsets.symmetric(horizontal: 20),
        alignment: alignment,
        child: Row(mainAxisSize: MainAxisSize.min, children: [Icon(icon, color: ink, size: 18), const SizedBox(width: 6), Text(label, style: TextStyle(color: ink, fontWeight: FontWeight.w600))]),
      );
}
