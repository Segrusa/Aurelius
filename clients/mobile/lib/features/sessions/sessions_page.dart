import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/protocol/protocol.dart';
import '../../data/tickets.dart' show needsMe;
import '../board/board_bloc.dart';
import '../board/board_page.dart' show BoardShownWatcher, needsMeOf;
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import 'session_rules.dart';
import 'sessions_bloc.dart';
import 'ticket_list_rules.dart';

/// Opens the Chat tab, where a session (or a ticket) is started.
void openNewSession(BuildContext context) => context.go('/chat');

/// The Sessions tab: your work sessions and the open tickets, one at a time
/// under a Sessions | Tickets switch ([tickets] opens on Tickets). Both lists
/// keep what is active on top, then the periods (Today, Yesterday, …), latest
/// activity first, as the desktop sidebar: opening one does not move it up.
class WorkSessionsPage extends StatelessWidget {
  const WorkSessionsPage({super.key, this.tickets = false});

  /// Open on the Tickets list (`/sessions?tab=tickets`).
  final bool tickets;

  @override
  Widget build(BuildContext context) => MultiBlocProvider(
        providers: [
          BlocProvider(create: (context) => SessionsBloc(context.read<AureliusRepository>())),
          BlocProvider(create: (context) {
            final company = context.read<CompanyBloc>();
            return BoardBloc(context.read<AureliusRepository>(), ticketTask: (id) => company.state.tasks.any((t) => t.id == id && t.ticketId != null));
          }),
        ],
        child: BoardShownWatcher(child: SessionsShownWatcher(child: _WorkSessionsBody(tickets: tickets))),
      );
}

/// Tells the [SessionsBloc] above whether its screen shows: the shell turns the
/// tickers of hidden tabs (and of pages under another one) off.
class SessionsShownWatcher extends StatefulWidget {
  const SessionsShownWatcher({super.key, required this.child});

  final Widget child;

  @override
  State<SessionsShownWatcher> createState() => _SessionsShownWatcherState();
}

class _SessionsShownWatcherState extends State<SessionsShownWatcher> {
  bool? _shown;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final shown = TickerMode.valuesOf(context).enabled;
    if (shown == _shown) return;
    _shown = shown;
    context.read<SessionsBloc>().add(SessionsShown(shown));
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

class _WorkSessionsBody extends StatefulWidget {
  const _WorkSessionsBody({required this.tickets});

  final bool tickets;

  @override
  State<_WorkSessionsBody> createState() => _WorkSessionsBodyState();
}

class _WorkSessionsBodyState extends State<_WorkSessionsBody> {
  late bool _tickets = widget.tickets;

  @override
  void didUpdateWidget(_WorkSessionsBody old) {
    super.didUpdateWidget(old);
    // A link to the tickets (an old Board place) switches the list.
    if (widget.tickets != old.tickets) _tickets = widget.tickets;
  }

  @override
  Widget build(BuildContext context) {
    final state = context.watch<SessionsBloc>().state;
    final board = context.watch<BoardBloc>().state;
    final company = context.watch<CompanyBloc>().state;
    final me = context.select<ConnectionBloc, String?>((b) => b.state.user?.id);
    bool mine(TicketView t) => needsMeOf(t, company, board, me: me);
    String? rootWaiting(TicketView t) => rootWaitingOf(t, company);
    final now = DateTime.now();
    final sessions = state.supported ? sessionSections(state.sessions, now) : <(SessionPeriod, List<WorkSessionView>)>[];
    final tickets = board.board == null ? <(SessionPeriod, List<TicketView>)>[] : ticketSections(board.tickets, now, needsYou: mine, rootWaiting: rootWaiting);
    final runningSessions = [for (final (p, list) in sessions) if (p == SessionPeriod.active) ...list];
    final runningTickets = [for (final (p, list) in tickets) if (p == SessionPeriod.active) ...list];
    final running = runningSessions.length + runningTickets.length;
    // Who works shows as the Sessions tab's badge, so the page has no count of its own.
    return LargeTitleScaffold(
      title: 'Sessions',
      onRefresh: () async {
        context.read<BoardBloc>().add(const BoardRefreshed());
        context.read<SessionsBloc>().add(const SessionsRefreshed());
      },
      header: [
        // What runs or waits for you now stands on top, whichever list the switch shows.
        if (running > 0) ...[
          GroupHeader('Running', key: const ValueKey('sessions-running'), count: running),
          InsetGroup(children: [
            for (final s in runningSessions) SessionRow(session: s),
            for (final t in runningTickets) TicketRow(ticket: t, needsYou: mine(t), rootWaiting: rootWaiting(t)),
          ]),
        ],
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 0),
          child: BrandSegments<bool>(
            key: const ValueKey('sessions-switch'),
            height: 34,
            segments: const [(false, 'Sessions'), (true, 'Tickets')],
            selected: _tickets,
            onChanged: (v) => setState(() => _tickets = v),
          ),
        ),
      ],
      fill: _tickets ? _ticketFill(context, board) : _fill(context, state),
      children: _tickets ? _ticketList(tickets, mine, rootWaiting) : _list(sessions),
    );
  }

  Widget? _ticketFill(BuildContext context, BoardState board) {
    if (board.board == null && board.error != null) {
      return EmptyState(
        'Could not load the tickets.',
        key: const ValueKey('tickets-error'),
        detail: board.error,
        action: OutlinedButton(onPressed: () => context.read<BoardBloc>().add(const BoardRefreshed()), child: const Text('Try again')),
      );
    }
    if (board.board == null) return const Center(child: CircularProgressIndicator());
    if (ticketSections(board.tickets, DateTime.now(), needsYou: (_) => false).isEmpty) {
      return EmptyState(
        'No tickets yet',
        key: const ValueKey('tickets-empty'),
        detail: 'Tickets from Todo up to Done show here.',
        action: FilledButton(onPressed: () => context.push('/board/new'), child: const Text('New ticket')),
      );
    }
    return null;
  }

  /// The sections below the switch; the active ones stand under Running on top.
  List<Widget> _ticketList(List<(SessionPeriod, List<TicketView>)> sections, bool Function(TicketView) mine, String? Function(TicketView) rootWaiting) => [
        for (final (period, tickets) in sections)
          if (period != SessionPeriod.active) ...[
          GroupHeader(period.label, key: ValueKey('tickets-section-${period.name}'), count: tickets.length),
          InsetGroup(children: [for (final t in tickets) TicketRow(ticket: t, needsYou: mine(t), rootWaiting: rootWaiting(t))]),
        ],
      ];

  Widget? _fill(BuildContext context, SessionsState state) {
    if (!state.supported) {
      return EmptyState(
        'Sessions are not on this server',
        key: const ValueKey('sessions-unsupported'),
        detail: 'Update Aurelius on the PC to start work sessions. You can still give work as a ticket.',
        action: FilledButton(onPressed: () => context.push('/board/new'), child: const Text('New ticket')),
      );
    }
    if (state.sessions.isEmpty && state.error != null) {
      return EmptyState(
        'Could not load your sessions.',
        key: const ValueKey('sessions-error'),
        detail: state.error,
        action: OutlinedButton(key: const ValueKey('sessions-retry'), onPressed: () => context.read<SessionsBloc>().add(const SessionsRefreshed()), child: const Text('Try again')),
      );
    }
    if (state.sessions.isEmpty && state.loading) return const Center(child: CircularProgressIndicator());
    if (state.sessions.isEmpty) {
      return EmptyState(
        'No sessions yet',
        key: const ValueKey('sessions-empty'),
        detail: 'Start one to work with an employee in a project, without a ticket.',
        action: FilledButton(onPressed: () => openNewSession(context), child: const Text('New session')),
      );
    }
    return null;
  }

  List<Widget> _list(List<(SessionPeriod, List<WorkSessionView>)> sections) => [
        for (final (period, sessions) in sections)
          if (period != SessionPeriod.active) ...[
          GroupHeader(period.label, key: ValueKey('sessions-section-${period.name}'), count: sessions.length),
          InsetGroup(children: [for (final s in sessions) SessionRow(session: s)]),
        ],
      ];
}

/// The `waiting_reason` of [t]'s main task, from the company's tasks; null when it is not loaded.
String? rootWaitingOf(TicketView t, CompanyState company) {
  final root = t.rootTaskId;
  if (root == null) return null;
  for (final task in company.tasks) {
    if (task.id == root) return task.waitingReason;
  }
  return null;
}

/// The bold-ish title of a session or ticket row.
TextStyle _rowTitle() => TextStyle(fontSize: 15, fontWeight: FontWeight.w500, color: Tokens.label);

/// One ticket: its number and title, owner and project (and Done once it is), and its signal.
class TicketRow extends StatelessWidget {
  const TicketRow({super.key, required this.ticket, required this.needsYou, this.rootWaiting});

  final TicketView ticket;

  /// The reader's ◆ ([needsMe]).
  final bool needsYou;

  /// The `waiting_reason` of its main task, when known: waiting for its
  /// delegated parts still counts as working.
  final String? rootWaiting;

  @override
  Widget build(BuildContext context) {
    final t = ticket;
    return GroupRow(
      key: ValueKey('ticket-row-${t.id}'),
      title: '#${t.number} ${t.title}',
      titleStyle: _rowTitle(),
      subtitle: '${t.column == ColumnKind.done ? 'Done · ' : ''}${t.ownerName ?? 'No owner yet'} · ${t.projectName} · ${Fmt.ago(t.updatedAtMs)}',
      leading: SessionMark.lead(ticketSignal(t, needsYou: needsYou, rootWaiting: rootWaiting)),
      chevron: true,
      onTap: () => context.push('/ticket/${t.id}'),
    );
  }
}

/// One session: its title, who and which project, and its signal.
class SessionRow extends StatelessWidget {
  const SessionRow({super.key, required this.session});

  final WorkSessionView session;

  @override
  Widget build(BuildContext context) {
    final s = session;
    return GroupRow(
      key: ValueKey('session-${s.conversationId}'),
      title: s.title.isEmpty ? 'Session' : s.title,
      titleStyle: _rowTitle(),
      subtitle: '${sessionWho(s)} · ${s.projectName} · ${Fmt.ago(s.lastActivityMs)}',
      leading: SessionMark.lead(sessionSignal(s)),
      chevron: true,
      // Opening it marks it seen on the server; read the list again on the way back.
      onTap: () async {
        final sessions = context.read<SessionsBloc>();
        await context.push('/session/${s.conversationId}');
        if (!sessions.isClosed) sessions.add(const SessionsRefreshed());
      },
    );
  }
}

/// The dot of a session's row: pulsing while it works, red while it waits for your
/// choice (as on the desktop), orange when it finished an answer you have not opened yet.
class SessionMark extends StatelessWidget {
  const SessionMark({super.key, required this.signal});

  final SessionSignal? signal;

  /// The mark in front of a row's title, or nothing when there is no signal
  /// (the title then starts at the edge, as in the design).
  static Widget? lead(SessionSignal? signal) => signal == null ? null : SessionMark(signal: signal);

  @override
  Widget build(BuildContext context) => switch (signal) {
        null => const SizedBox.shrink(),
        SessionSignal.working => Semantics(label: 'Working', child: StatusDot(key: const ValueKey('session-working'), color: Tokens.accent, pulse: true)),
        SessionSignal.needsYou => Semantics(label: 'Waiting for your choice', child: StatusDot(key: const ValueKey('session-needs-you'), color: Tokens.red)),
        SessionSignal.done => Semantics(label: 'Finished, not opened yet', child: StatusDot(key: const ValueKey('session-done'), color: Tokens.orange)),
      };
}
