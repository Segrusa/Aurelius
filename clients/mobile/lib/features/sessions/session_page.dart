import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/protocol/protocol.dart';
import '../chat/chat_bloc.dart';
import '../chat/chat_view.dart';
import '../company/company_bloc.dart';
import '../connection/connection_banner.dart';
import 'session_rules.dart';
import 'sessions_bloc.dart';
import 'sessions_page.dart';

/// `/session/:cid` (ticket #71): one work session. The conversation through
/// the chat view (later messages go in with `chat.send`, with live steps and
/// Stop), a status line, and while the dispatcher has not given it to anyone
/// the choice of who takes it (`work_session.assign`) in the composer's place.
class SessionPage extends StatelessWidget {
  const SessionPage({super.key, required this.conversationId});

  final String conversationId;

  @override
  Widget build(BuildContext context) => BlocProvider(
        create: (context) => SessionsBloc(context.read<AureliusRepository>()),
        child: SessionsShownWatcher(child: _SessionBody(conversationId: conversationId)),
      );
}

class _SessionBody extends StatelessWidget {
  const _SessionBody({required this.conversationId});

  final String conversationId;

  @override
  Widget build(BuildContext context) {
    final state = context.watch<SessionsBloc>().state;
    final session = state.byId(conversationId);
    if (session == null) return _Missing(state: state);
    return Scaffold(
      appBar: GlassAppBar(
        titleSpacing: 0,
        title: Text(session.title.isEmpty ? 'Session' : session.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: Tokens.title(size: 18)),
      ),
      body: Column(children: [
        const ConnectionBanner(),
        Expanded(
          // A new employee (after Choose who takes it) gets a new bloc: messages go to them.
          child: BlocProvider(
            key: ValueKey('session-$conversationId-${session.employeeId}'),
            create: (context) => ChatBloc(context.read<AureliusRepository>(), conversationId: conversationId, employeeId: session.employeeId, session: true),
            child: Column(children: [
              SessionStatusLine(session: session),
              Expanded(
                child: ChatView(
                  key: ValueKey('session-chat-$conversationId'),
                  emptyTitle: 'This session is starting.',
                  emptyDetail: 'The first answer appears here. Talk with the employee as in a chat; there is no ticket.',
                  composer: session.dispatch == null,
                  readOnlyNote: chooserText(session),
                  readOnlyAction: session.dispatch == null ? null : _ChooseWho(session: session),
                ),
              ),
            ]),
          ),
        ),
      ]),
    );
  }
}

/// A session the list does not have (yet): loading, its error with Try again, or gone.
class _Missing extends StatelessWidget {
  const _Missing({required this.state});

  final SessionsState state;

  @override
  Widget build(BuildContext context) {
    final Widget body;
    if (!state.supported) {
      body = EmptyState(
        'Sessions are not on this server',
        key: const ValueKey('session-unsupported'),
        action: FilledButton(onPressed: () => context.go('/sessions?tab=tickets'), child: const Text('Open tickets')),
      );
    } else if (state.error != null) {
      body = EmptyState(
        'Could not load this session.',
        key: const ValueKey('session-error'),
        detail: state.error,
        action: OutlinedButton(key: const ValueKey('session-retry'), onPressed: () => context.read<SessionsBloc>().add(const SessionsRefreshed()), child: const Text('Try again')),
      );
    } else if (state.loading) {
      body = const Center(child: CircularProgressIndicator());
    } else {
      body = EmptyState(
        'This session no longer exists',
        key: const ValueKey('session-gone'),
        action: FilledButton(onPressed: () => context.go('/sessions'), child: const Text('Go to Sessions')),
      );
    }
    return Scaffold(appBar: const GlassAppBar(title: Text('Session')), body: body);
  }
}

/// The session's status (desktop's Status row). Stop is the chat view's own, on the answer and in the composer.
class SessionStatusLine extends StatelessWidget {
  const SessionStatusLine({super.key, required this.session});

  final WorkSessionView session;

  @override
  Widget build(BuildContext context) {
    final runId = context.select<ChatBloc, String?>((b) => b.state.streaming?.runId);
    final running = session.running || runId != null;
    return Container(
      key: const ValueKey('session-status'),
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
      decoration: BoxDecoration(border: Border(bottom: BorderSide(color: Tokens.border, width: 0.5))),
      child: Row(children: [
        SessionMark(signal: runId != null ? SessionSignal.working : sessionSignal(session)),
        const SizedBox(width: 8),
        Expanded(child: Text(sessionStatus(running, session.dispatch), key: const ValueKey('session-status-text'), style: TextStyle(fontSize: 13, color: Tokens.label2))),
      ]),
    );
  }
}

/// "Choose who takes it" (`work_session.assign`): the active employees; the chosen one answers at once.
class _ChooseWho extends StatefulWidget {
  const _ChooseWho({required this.session});

  final WorkSessionView session;

  @override
  State<_ChooseWho> createState() => _ChooseWhoState();
}

class _ChooseWhoState extends State<_ChooseWho> {
  bool _busy = false;

  Future<void> _choose() async {
    final employees = context.read<CompanyBloc>().state.activeEmployees;
    final picked = await showModalBottomSheet<String>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
          child: ListView(shrinkWrap: true, children: [
            const Padding(padding: EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow('Who takes this session')),
            for (final e in employees)
              ListTile(key: ValueKey('assign-${e.id}'), title: Text(e.name), subtitle: Text(e.title ?? e.role), onTap: () => Navigator.pop(sheet, e.id)),
          ]),
        ),
      ),
    );
    if (picked == null || !mounted) return;
    final repository = context.read<AureliusRepository>();
    final sessions = context.read<SessionsBloc>();
    setState(() => _busy = true);
    try {
      await repository.write(workSessionAssign(AssignWorkSession(conversationId: widget.session.conversationId, employeeId: picked)), retry: false);
      sessions.add(const SessionsRefreshed());
    } on ProtocolException catch (e) {
      if (mounted) toast(context, e.shown);
    } on Object catch (e) {
      if (mounted) toast(context, '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final nobody = widget.session.dispatch == SessionDispatch.nobody;
    final child = Text(_busy ? 'Choosing…' : 'Choose who takes it');
    final onPressed = _busy ? null : _choose;
    return nobody
        ? FilledButton(key: const ValueKey('session-choose'), onPressed: onPressed, child: child)
        : OutlinedButton(key: const ValueKey('session-choose'), onPressed: onPressed, child: child);
  }
}
