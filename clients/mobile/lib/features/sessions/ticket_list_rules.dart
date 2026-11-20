// The Tickets list of the Sessions tab, pure. Ported from the desktop
// sidebar's sidebarTickets and ticketSignal (ticket-session-logic.ts); the
// sections are the mobile ones of the Sessions list (session_rules.dart).

import '../../data/protocol/protocol.dart';
import 'session_rules.dart';

/// The columns where a ticket is still open.
const openTicketColumns = {ColumnKind.todo, ColumnKind.doing, ColumnKind.planReview, ColumnKind.testing, ColumnKind.review};

/// Whether [t] belongs in the Tickets list: not archived, and open or on Done
/// (ticket #75), as the Sessions list keeps a finished session. The Backlog stays out.
bool listedTicket(TicketView t) => t.archivedAtMs == null && (t.column == ColumnKind.done || openTicketColumns.contains(t.column));

/// What a ticket's row signals: it needs you (the reader's ◆), else it works
/// while its main task is queued or running, or waits for the parts it
/// delegated ([rootWaiting], the main task's `waiting_reason`): the ticket is
/// still under way then (desktop ticketSignal).
SessionSignal? ticketSignal(TicketView t, {required bool needsYou, String? rootWaiting}) {
  if (needsYou) return SessionSignal.needsYou;
  final status = t.rootTaskStatus;
  if (status == TaskStatus.running || status == TaskStatus.queued) return SessionSignal.working;
  if (status == TaskStatus.waiting && rootWaiting == 'dependency') return SessionSignal.working;
  return null;
}

/// The open tickets in their sections, each last updated first: opening one
/// does not move it up. [needsYou] is the reader's ◆ of a ticket ([needsMe]);
/// [rootWaiting] the `waiting_reason` of its main task, when known.
List<(SessionPeriod, List<TicketView>)> ticketSections(
  Iterable<TicketView> tickets,
  DateTime now, {
  required bool Function(TicketView) needsYou,
  String? Function(TicketView)? rootWaiting,
}) =>
    sectionsByUse(
      tickets.where(listedTicket),
      now,
      usedAt: (t) => t.updatedAtMs,
      active: (t) => ticketSignal(t, needsYou: needsYou(t), rootWaiting: rootWaiting?.call(t)) != null,
    );
