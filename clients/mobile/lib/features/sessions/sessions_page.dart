import 'package:flutter/cupertino.dart';
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
import 'recent_use.dart';
import 'session_rules.dart';
import 'sessions_bloc.dart';
import 'ticket_list_rules.dart';

/// Opens the Chat tab, where a session (or a ticket) is started.
void openNewSession(BuildContext context) => context.go('/chat');

/// The Sessions tab: your work sessions and the open tickets, one at a time
/// under a Sessions | Tickets switch ([tickets] opens on Tickets). Both lists
/// keep what is active on top, then the periods (Today, Yesterday, …), most
/// recently used first, as the desktop sidebar: the later of the activity and
/// when you last opened it on this phone.
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

/// "2 working": the header's count; none while nobody works.
String? workingLabel(int working) => working > 0 ? '$working working' : null;

class _WorkSessionsBody extends StatefulWidget {
  const _WorkSessionsBody({required this.tickets});

  final bool tickets;

  @override
  State<_WorkSessionsBody> createState() => _WorkSessionsBodyState();
}

class _WorkSessionsBodyState extends State<_WorkSessionsBody> {
  late bool _tickets = widget.tickets;
  Map<String, int> _uses = const {};
  bool? _shown;

  @override
  void didUpdateWidget(_WorkSessionsBody old) {
    super.didUpdateWidget(old);
    // A link to the tickets (an old Board place) switches the list.
    if (widget.tickets != old.tickets) _tickets = widget.tickets;
  }

  /// The open times are read again each time the tab shows: a session or ticket opened meanwhile moves up.
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final shown = TickerMode.valuesOf(context).enabled;
    if (shown == _shown) return;
    _shown = shown;
    if (shown) _loadUses();
  }

  Future<void> _loadUses() async {
    final uses = await loadUsesOn(context.read<AureliusRepository>());
    if (mounted) setState(() => _uses = uses);
  }

  @override
  Widget build(BuildContext context) {
    final state = context.watch<SessionsBloc>().state;
    final board = context.watch<BoardBloc>().state;
    final company = context.watch<CompanyBloc>().state;
    final me = context.select<ConnectionBloc, String?>((b) => b.state.user?.id);
    bool mine(TicketView t) => needsMeOf(t, company, board, me: me);
    final working = workingLabel(state.working);
    final needYou = board.tickets.where((t) => listedTicket(t) && mine(t)).length;
    final count = _tickets ? (needYou > 0 ? '$needYou need${needYou == 1 ? 's' : ''} you' : null) : working;
    return Scaffold(
      appBar: GlassAppBar(
        title: Row(mainAxisSize: MainAxisSize.min, children: [
          const Text('Sessions'),
          if (count != null) ...[
            const SizedBox(width: 10),
            StatusDot(color: _tickets ? Tokens.warning : Tokens.accent, pulse: !_tickets, size: 7),
            const SizedBox(width: 4),
            Flexible(child: Text(count, key: ValueKey(_tickets ? 'tickets-need-you' : 'sessions-working'), overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 13, color: Tokens.label2, fontWeight: FontWeight.w400))),
          ],
        ]),
        actions: [
          IconButton(key: const ValueKey('sessions-new'), tooltip: 'New session', icon: const Icon(CupertinoIcons.square_pencil), onPressed: () => openNewSession(context)),
        ],
      ),
      body: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 4),
          child: CupertinoSlidingSegmentedControl<bool>(
            key: const ValueKey('sessions-switch'),
            groupValue: _tickets,
            children: const {
              false: Padding(padding: EdgeInsets.symmetric(vertical: 6), child: Text('Sessions')),
              true: Padding(padding: EdgeInsets.symmetric(vertical: 6), child: Text('Tickets')),
            },
            onValueChanged: (v) => setState(() => _tickets = v ?? false),
          ),
        ),
        Expanded(child: _tickets ? _ticketList(context, board, mine) : _list(context, state)),
      ]),
    );
  }

  Widget _ticketList(BuildContext context, BoardState board, bool Function(TicketView) mine) {
    if (board.board == null && board.error != null) {
      return EmptyState(
        'Could not load the tickets.',
        key: const ValueKey('tickets-error'),
        detail: board.error,
        action: OutlinedButton(onPressed: () => context.read<BoardBloc>().add(const BoardRefreshed()), child: const Text('Try again')),
      );
    }
    if (board.board == null) return const Center(child: CircularProgressIndicator());
    final sections = ticketSections(board.tickets, DateTime.now(), needsYou: mine, uses: _uses);
    if (sections.isEmpty) {
      return EmptyState(
        'No open tickets',
        key: const ValueKey('tickets-empty'),
        detail: 'Tickets in Todo, Doing, Plan review, Testing or Review show here.',
        action: FilledButton(onPressed: () => context.push('/board/new'), child: const Text('New ticket')),
      );
    }
    return RefreshIndicator(
      onRefresh: () async => context.read<BoardBloc>().add(const BoardRefreshed()),
      child: ListView(
        padding: EdgeInsets.fromLTRB(0, 8, 0, listBottom(context)),
        children: [
          for (final (period, tickets) in sections) ...[
            GroupHeader(period.label, key: ValueKey('tickets-section-${period.name}'), count: tickets.length),
            InsetGroup(children: [for (final t in tickets) TicketRow(ticket: t, needsYou: mine(t))]),
          ],
        ],
      ),
    );
  }

  Widget _list(BuildContext context, SessionsState state) {
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
    return RefreshIndicator(
      onRefresh: () async => context.read<SessionsBloc>().add(const SessionsRefreshed()),
      child: ListView(
        padding: EdgeInsets.fromLTRB(0, 8, 0, listBottom(context)),
        children: [
          for (final (period, sessions) in sessionSections(state.sessions, DateTime.now(), uses: _uses)) ...[
            GroupHeader(period.label, key: ValueKey('sessions-section-${period.name}'), count: sessions.length),
            InsetGroup(children: [for (final s in sessions) SessionRow(session: s)]),
          ],
        ],
      ),
    );
  }
}

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
      subtitle: '${t.ownerName ?? 'No owner yet'} · ${t.projectName} · ${Fmt.ago(t.updatedAtMs)}',
      trailing: SessionMark(signal: ticketSignal(t, needsYou: needsYou)),
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
      subtitle: '${sessionWho(s)} · ${s.projectName} · ${Fmt.ago(s.lastActivityMs)}',
      trailing: SessionMark(signal: sessionSignal(s)),
      chevron: true,
      onTap: () => context.push('/session/${s.conversationId}'),
    );
  }
}

/// The dot of a session's row: pulsing while it works, the warning colour while it waits for your choice.
class SessionMark extends StatelessWidget {
  const SessionMark({super.key, required this.signal});

  final SessionSignal? signal;

  @override
  Widget build(BuildContext context) => switch (signal) {
        null => const SizedBox.shrink(),
        SessionSignal.working => Semantics(label: 'Working', child: StatusDot(key: const ValueKey('session-working'), color: Tokens.accent, pulse: true)),
        SessionSignal.needsYou => Semantics(label: 'Waiting for your choice', child: StatusDot(key: const ValueKey('session-needs-you'), color: Tokens.warning)),
      };
}
