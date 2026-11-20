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
    // Who works shows as the Sessions tab's badge, so the page has no count of its own.
    return LargeTitleScaffold(
      title: 'Sessions',
      onRefresh: () async => _tickets ? context.read<BoardBloc>().add(const BoardRefreshed()) : context.read<SessionsBloc>().add(const SessionsRefreshed()),
      header: [
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
      children: _tickets ? _ticketList(board, mine) : _list(state),
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
        'No open tickets',
        key: const ValueKey('tickets-empty'),
        detail: 'Tickets in Todo, Doing, Plan review, Testing or Review show here.',
        action: FilledButton(onPressed: () => context.push('/board/new'), child: const Text('New ticket')),
      );
    }
    return null;
  }

  List<Widget> _ticketList(BoardState board, bool Function(TicketView) mine) => [
        for (final (period, tickets) in ticketSections(board.tickets, DateTime.now(), needsYou: mine)) ...[
          GroupHeader(period.label, key: ValueKey('tickets-section-${period.name}'), count: tickets.length),
          InsetGroup(children: [for (final t in tickets) TicketRow(ticket: t, needsYou: mine(t))]),
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

  List<Widget> _list(SessionsState state) => [
        for (final (period, sessions) in sessionSections(state.sessions, DateTime.now())) ...[
          GroupHeader(period.label, key: ValueKey('sessions-section-${period.name}'), count: sessions.length),
          InsetGroup(children: [for (final s in sessions) SessionRow(session: s)]),
        ],
      ];
}

/// The bold-ish title of a session or ticket row.
TextStyle _rowTitle() => TextStyle(fontSize: 15, fontWeight: FontWeight.w500, color: Tokens.label);

/// One open ticket: its number and title, owner and project, and its signal.
class TicketRow extends StatelessWidget {
  const TicketRow({super.key, required this.ticket, required this.needsYou});

  final TicketView ticket;

  /// The reader's ◆ ([needsMe]).
  final bool needsYou;

  @override
  Widget build(BuildContext context) {
    final t = ticket;
    return GroupRow(
      key: ValueKey('ticket-row-${t.id}'),
      title: '#${t.number} ${t.title}',
      titleStyle: _rowTitle(),
      subtitle: '${t.ownerName ?? 'No owner yet'} · ${t.projectName} · ${Fmt.ago(t.updatedAtMs)}',
      leading: SessionMark.lead(ticketSignal(t, needsYou: needsYou)),
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
      onTap: () => context.push('/session/${s.conversationId}'),
    );
  }
}

/// The dot of a session's row: pulsing while it works, the warning colour while it waits for your choice.
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
        SessionSignal.needsYou => Semantics(label: 'Waiting for your choice', child: StatusDot(key: const ValueKey('session-needs-you'), color: Tokens.warning)),
      };
}
