import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/net/protocol.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../chat/chat_bloc.dart';
import '../chat/chat_view.dart';
import '../company/company_bloc.dart';
import '../runtime/runtime_bloc.dart';
import '../../data/run_config.dart';
import '../../data/views.dart';
import 'session_chrome.dart';
import 'session_rules.dart';
import 'session_screen_rules.dart';
import 'sessions_bloc.dart';
import 'sessions_page.dart';

/// `/session/:cid` (ticket #71, design "Aurelius sessiescherm"): one work
/// session. The conversation fills the screen under a glass head and a
/// floating composer (later messages go in with `chat.send`, with live steps
/// and Stop); its status and run are in Properties behind "…". While the
/// dispatcher has not given it to anyone, the choice of who takes it
/// (`work_session.assign`) stands in the composer's place.
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
    // A new employee (after Choose who takes it) gets a new bloc: messages go to them.
    return BlocProvider(
      key: ValueKey('session-$conversationId-${session.employeeId}'),
      create: (context) => ChatBloc(context.read<AureliusRepository>(), conversationId: conversationId, employeeId: session.employeeId, session: true),
      child: Builder(
        builder: (context) => SessionScaffold(
          head: SessionHead(
            title: session.title.isEmpty ? 'Session' : session.title,
            titleKey: const ValueKey('session-title'),
            kind: SessionKind.session,
            onMore: (_) => showSessionProperties(context, session),
          ),
          body: ChatView(
            key: ValueKey('session-chat-$conversationId'),
            floating: true,
            topInset: sessionHeadInset(context),
            emptyTitle: 'This session is starting.',
            emptyDetail: 'The first answer appears here. Talk with the employee as in a chat; there is no ticket.',
            composer: session.dispatch == null,
            hint: session.dispatch == null ? 'Reply to ${session.employeeName}' : null,
            readOnlyNote: chooserText(session),
            readOnlyAction: session.dispatch == null ? null : _ChooseWho(session: session),
          ),
        ),
      ),
    );
  }
}

/// The session's Properties (desktop: its side panel): who, its status, where,
/// what it runs on and since when; then the files its runs wrote and the
/// guidelines of its latest run.
Future<void> showSessionProperties(BuildContext context, WorkSessionView session) async {
  final chat = context.read<ChatBloc>();
  final repository = context.read<AureliusRepository>();
  final run = ConversationRun.of(context, runtime: context.read<RuntimeBloc>().state, chat: chat.state, company: context.read<CompanyBloc>().state);
  final messages = chat.state.messages;
  final running = session.running || chat.state.streaming != null;
  final files = filesWritten(messages);
  final runId = lastRunOf(messages);
  // The guidelines are read once per opening; a server without them shows none.
  List<RunGuideline> guidelines = const [];
  if (runId != null) {
    try {
      guidelines = await repository.guidelinesOfRun(runId);
    } on Object {
      guidelines = const [];
    }
  }
  if (!context.mounted) return;
  final model = run?.effectiveModel;
  final effort = run?.effectiveEffort;
  await showPropertiesSheet(
    context,
    rows: [
      ('Employee', Text(session.dispatch == null ? session.employeeName : 'Not chosen yet', key: const ValueKey('session-prop-employee'))),
      ('Status', StatusChip(key: const ValueKey('session-status'), tone: sessionTone(running, session.dispatch), text: sessionStatus(running, session.dispatch))),
      ('Project', Text(session.projectName)),
      if (session.branch case final branch?) ('Branch', Text(branch, maxLines: 1, overflow: TextOverflow.ellipsis)),
      ('Folder', Text(sessionFolder(session), maxLines: 2, overflow: TextOverflow.ellipsis)),
      ('Model', Text(model == null ? 'Default' : ModelCatalog.friendly(model))),
      if (run?.effortShown ?? true) ('Effort', Text(effort == null ? 'Default' : ModelCatalog.effortLabel(effort))),
      ('Mode', Text(chat.state.mode.label)),
      ('Started', Text(startedLabel(session.createdAtMs), style: const TextStyle(fontFeatures: [FontFeature.tabularFigures()]))),
    ],
    links: [
      if (files.isNotEmpty)
        PropertiesLink('Files written', files.length, () => showPropertiesSheet(context, title: 'Files written', body: SheetList(entries: [for (final f in files) (f, null)]))),
      if (guidelines.isNotEmpty)
        PropertiesLink('Guidelines in this run', guidelines.length, () => showPropertiesSheet(context, title: 'Guidelines in this run', body: SheetList(entries: [for (final g in guidelines) (g.title, 'v${g.version}')]))),
    ],
  );
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
