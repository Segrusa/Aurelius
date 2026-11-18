import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/protocol/protocol.dart';
import 'session_rules.dart';
import 'sessions_bloc.dart';

/// Opens New session; on a server without work sessions, New ticket in its place.
void openNewSession(BuildContext context) {
  final sessions = context.read<AureliusRepository>().linkState.has(workSessionsFeature);
  context.push(sessions ? '/sessions/new' : '/board/new');
}

/// The Sessions tab (ticket #71): the reader's work sessions in sections
/// (Active, Today, Yesterday, This week, This month, Older), newest activity
/// first in each. A pulsing dot while one works, a warning dot while the dispatcher
/// chose nobody; the header counts how many work right now.
class WorkSessionsPage extends StatelessWidget {
  const WorkSessionsPage({super.key});

  @override
  Widget build(BuildContext context) => BlocProvider(
        create: (context) => SessionsBloc(context.read<AureliusRepository>()),
        child: const SessionsShownWatcher(child: _WorkSessionsBody()),
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

class _WorkSessionsBody extends StatelessWidget {
  const _WorkSessionsBody();

  @override
  Widget build(BuildContext context) {
    final state = context.watch<SessionsBloc>().state;
    final working = workingLabel(state.working);
    return Scaffold(
      appBar: GlassAppBar(
        title: Row(mainAxisSize: MainAxisSize.min, children: [
          const Text('Sessions'),
          if (working != null) ...[
            const SizedBox(width: 10),
            StatusDot(color: Tokens.accent, pulse: true, size: 7),
            const SizedBox(width: 4),
            Text(working, key: const ValueKey('sessions-working'), style: TextStyle(fontSize: 13, color: Tokens.label2, fontWeight: FontWeight.w400)),
          ],
        ]),
        actions: [
          IconButton(key: const ValueKey('sessions-new'), tooltip: 'New session', icon: const Icon(CupertinoIcons.square_pencil), onPressed: () => openNewSession(context)),
        ],
      ),
      body: _list(context, state),
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
          for (final (period, sessions) in sessionSections(state.sessions, DateTime.now())) ...[
            GroupHeader(period.label, key: ValueKey('sessions-section-${period.name}'), count: sessions.length),
            InsetGroup(children: [for (final s in sessions) SessionRow(session: s)]),
          ],
        ],
      ),
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
