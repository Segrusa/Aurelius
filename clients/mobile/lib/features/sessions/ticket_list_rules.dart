// The Tickets list of the Sessions tab, pure. Ported from the desktop
// sidebar's sidebarTickets and ticketSignal (ticket-session-logic.ts); the
// sections are the mobile ones of the Sessions list (session_rules.dart).

import '../../data/protocol/protocol.dart';
import 'recent_use.dart';
import 'session_rules.dart';

/// The columns where a ticket is still open: the list leaves Backlog and Done out.
const openTicketColumns = {ColumnKind.todo, ColumnKind.doing, ColumnKind.planReview, ColumnKind.testing, ColumnKind.review};

/// Whether [t] belongs in the Tickets list: not archived and in an open column.
bool listedTicket(TicketView t) => t.archivedAtMs == null && openTicketColumns.contains(t.column);

/// What a ticket's row signals: it needs you (the reader's ◆), else it works
/// while its main task is queued or running (desktop ticketSignal).
SessionSignal? ticketSignal(TicketView t, {required bool needsYou}) {
  if (needsYou) return SessionSignal.needsYou;
  final status = t.rootTaskStatus;
  if (status == TaskStatus.running || status == TaskStatus.queued) return SessionSignal.working;
  return null;
}

/// The open tickets in their sections, each most recently used first: the
/// later of their last update and when you last opened them on this phone.
/// [needsYou] is the reader's ◆ of a ticket ([needsMe]).
List<(SessionPeriod, List<TicketView>)> ticketSections(Iterable<TicketView> tickets, DateTime now, {required bool Function(TicketView) needsYou, Map<String, int> uses = const {}}) =>
    sectionsByUse(
      tickets.where(listedTicket),
      now,
      usedAt: (t) => lastUsed(uses, ticketUseKey(t.id), t.updatedAtMs),
      active: (t) => ticketSignal(t, needsYou: needsYou(t)) != null,
    );
