import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import '../activity/activity_chip.dart';

import '../../core/net/connection.dart' show LinkState;
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/channels.dart' show ticketChannelsFeature;
import '../../data/discussion.dart';
import '../../data/links.dart';
import '../../data/quick_task.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import '../chat/chat_bloc.dart';
import '../chat/chat_view.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../project/new_task_page.dart' show maxTaskFiles;
import '../links/link_actions.dart';
import '../markdown/links.dart';
import '../chat/run_author.dart';
import 'ticket_actions.dart';
import '../sessions/recent_use.dart';
import 'ticket_bloc.dart';
import 'ticket_gate.dart';
import 'ticket_properties.dart';
import 'ticket_session_rules.dart';
import 'ticket_text_dialog.dart';
import 'ticket_card.dart' show QuickTag;

/// `/ticket/:tid` (ticket #71, after the desktop's session of ticket #70): a
/// ticket as one session. Its conversation is the main view; the head says
/// #N, the title, the column and who; the gate of its column sits right above
/// the input, and the ticket's data is in a sheet behind the head's button.
/// [tid] is the ticket's id or its number (`/ticket/70`).
class TicketPage extends StatelessWidget {
  const TicketPage({super.key, required this.ticketId, this.discussion = false, this.activity = false, this.message});

  final String ticketId;

  /// An old `?tab=discussion`: the session is the discussion, so nothing changes.
  final bool discussion;

  /// An old `?tab=activity` (the `+n` of a row in the Inbox): opens the Activity sheet.
  final bool activity;

  /// A link to a message of its conversation: scroll to it there.
  final String? message;

  @override
  Widget build(BuildContext context) => BlocProvider(
        // Another ticket on the same route (a `go`) gets its own bloc, not the last one's.
        key: ValueKey('ticket-$ticketId'),
        create: (context) {
          final repository = context.read<AureliusRepository>();
          // Opened here: the Tickets list puts it up front (recent_use.dart).
          unawaited(noteUseOn(repository, ticketUseKey(ticketId)));
          return TicketBloc(repository, ticketId);
        },
        child: _TicketView(key: ValueKey('ticket-view-$ticketId'), activity: activity, message: message),
      );
}

/// The conversation of [ticket]'s session, when the server has one for it
/// (`ticket_channels`, or the older `ticket_discussion`, and its
/// `conversation_id`); null: "This server has no ticket conversation."
String? discussionOf(TicketView ticket, LinkState link) => link.has(ticketChannelsFeature) || link.has(ticketDiscussionFeature) ? ticket.conversationId : null;

/// The server takes files on an existing ticket (`ticket.update { add_files }`, ticket #3).
const ticketAddFilesFeature = 'ticket_add_files';

/// How many files can still be pasted into the ticket's title or description:
/// none when the server cannot add them to an existing ticket.
int ticketFileRoom(TicketView ticket, LinkState link) => link.has(ticketAddFilesFeature) ? (maxTaskFiles - ticket.attachments.length).clamp(0, maxTaskFiles) : 0;

/// The `ticket.update` for an edit of the [title] or the description: only the
/// text when it changed (an empty description clears it) and the pasted files;
/// `null` when nothing changed or a title would be empty.
Map<String, dynamic>? ticketTextUpdate(TicketView ticket, TicketTextEdit edit, {required bool title}) {
  final before = title ? ticket.title : ticket.description ?? '';
  final text = edit.text == before ? null : edit.text;
  if ((title && edit.text.isEmpty) || (text == null && edit.files.isEmpty)) return null;
  final files = edit.files.isEmpty ? null : edit.files;
  return title ? ticketUpdate(ticketId: ticket.id, title: text, addFiles: files) : ticketUpdate(ticketId: ticket.id, description: text, addFiles: files);
}

/// The board's name of a column.
String columnName(ColumnKind column) => switch (column) {
      ColumnKind.backlog => 'Backlog',
      ColumnKind.todo => 'Todo',
      ColumnKind.doing => 'Doing',
      ColumnKind.planReview => 'Plan review',
      ColumnKind.testing => 'Testing',
      ColumnKind.review => 'Review',
      ColumnKind.done => 'Done',
    };

class _TicketView extends StatefulWidget {
  const _TicketView({super.key, this.activity = false, this.message});

  final bool activity;
  final String? message;

  @override
  State<_TicketView> createState() => _TicketViewState();
}

class _TicketViewState extends State<_TicketView> {
  /// The input: Request changes and Send back take their words from it.
  final _composer = TextEditingController();

  /// An old `?tab=activity` opened its sheet once.
  late bool _landed = !widget.activity;

  @override
  void dispose() {
    _composer.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final page = context.watch<TicketBloc>().state;
    final company = context.watch<CompanyBloc>().state;
    final link = context.watch<ConnectionBloc>().state;
    final online = link.isOnline;
    final ticket = page.ticket;
    if (ticket == null) return _unreachable(context, page);
    if (!_landed) {
      _landed = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) showTicketProperties(context, activity: true);
      });
    }
    final conversation = discussionOf(ticket, link);
    final root = page.root;
    final me = link.user?.id;
    final noDispatcher = effectiveDispatcher(company.project(ticket.projectId), company.company?.dispatcherEmployeeId, company.employees) == null;
    final state = ticketState(ticket, root: root, needsOwner: ticket.column == ColumnKind.todo && ticket.ownerEmployeeId == null && (page.gaveUp || noDispatcher), ownerUnknown: page.activityFailed, me: me);
    final owner = ticket.ownerName ?? employeeName(company, ticket.ownerEmployeeId);
    final readOnly = sessionReadOnly(ticket);
    // The ◆ is the responsible's (B6); the gate's buttons stay for anyone.
    final mine = needsMe(ticket, state, me: me);
    final status = state == TicketState.done
        ? null
        : state.line(owner: owner, dispatcher: dispatcherName(company, ticket.projectId), tester: testerName(company), login: ticket.loginUsername, mine: ticket.loginUser == me, untilMs: root?.waitingUntilMs, expired: link.claudeLogin?.status == ClaudeLoginStatus.expired, others: decidedByOther(ticket, state, me: me));
    // Yours to do or not, anyone can decide (B6, §1.4).
    final othersNote = state.needsYou && !needsMe(ticket, state, me: me) ? '${responsibleLabel(ticket)} is responsible. You can decide too.' : null;
    Widget gate({required bool input}) => TicketGateView(page: page, state: state, online: online, composer: input ? _composer : null, note: othersNote);
    final scaffold = Scaffold(
      appBar: GlassAppBar(
        title: Text('#${ticket.number} · ${ticket.projectName}'),
        actions: [
          IconButton(
            key: const ValueKey('ticket-properties'),
            tooltip: 'Ticket properties',
            icon: const Icon(Icons.view_sidebar_outlined),
            onPressed: () => showTicketProperties(context),
          ),
          _moreMenu(context, ticket, root, conversation, online),
        ],
      ),
      body: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _Head(ticket: ticket, owner: owner, me: me, quick: isQuickTask(ticket, on: quickTasksOn(link)), status: status, mine: mine),
        Expanded(
          child: conversation == null
              ? Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                  const Expanded(child: EmptyState('This server has no ticket conversation.', key: ValueKey('ticket-no-conversation'))),
                  gate(input: false),
                  if (readOnly != null) _ReadOnlyBar(text: readOnly) else SizedBox(height: MediaQuery.paddingOf(context).bottom),
                ])
              : ChatView(
                  key: ValueKey('discussion-${ticket.id}'),
                  revealMessage: widget.message,
                  emptyTitle: 'Discuss the ticket here',
                  emptyDetail: sessionHint(ticket, owner: owner),
                  composer: readOnly == null,
                  readOnlyNote: readOnly,
                  hint: sessionPlaceholder(ticket, owner: owner),
                  source: TicketSource(from: 'Discussion of #${ticket.number}', projectId: ticket.projectId),
                  taskProjectId: ticket.projectId,
                  composerText: _composer,
                  aboveComposer: readOnly == null ? gate(input: true) : null,
                  composerActions: (context, _) => TicketUnderInput(ticket: ticket, online: online, owner: owner),
                ),
        ),
      ]),
    );
    if (conversation == null) return scaffold;
    // One chat for the conversation while the session is open; another conversation gets its own.
    return BlocProvider(
      key: ValueKey('discussion-$conversation'),
      create: (context) => ChatBloc.discussion(context.read<AureliusRepository>(), ticketId: ticket.id, conversationId: conversation, runAuthor: (runId) => runAuthorOf(context.read<CompanyBloc>().state, runId)),
      child: scaffold,
    );
  }

  /// No ticket to show: not found ("Ticket #70 not found."), no access, the
  /// error with Try again, or the first read; never an endless spinner.
  Widget _unreachable(BuildContext context, TicketPageState page) {
    final number = page.number;
    return Scaffold(
      appBar: GlassAppBar(title: Text(number == null ? 'Ticket' : '#$number')),
      body: page.notFound || page.denied
          ? EmptyState(
              page.notFound ? (number == null ? 'Ticket not found' : 'Ticket #$number not found.') : 'No access',
              key: const ValueKey('ticket-unreachable'),
              detail: page.notFound ? "This ticket doesn't exist on ${context.read<AureliusRepository>().host.name}. It may have been deleted from the Backlog." : "You don't have access to this ticket.",
              action: FilledButton(key: const ValueKey('ticket-go-to-board'), onPressed: () => context.go('/sessions?tab=tickets'), child: const Text('Open tickets')),
            )
          : page.error != null
              ? ErrorRetry(page.error!, onRetry: () => context.read<TicketBloc>().add(const TicketRefreshed()))
              : const Center(child: CircularProgressIndicator()),
    );
  }

  Widget _moreMenu(BuildContext context, TicketView ticket, TaskView? root, String? conversation, bool online) => GlassMenuButton<String>(
        tooltip: 'Ticket actions',
        items: () => [
          GlassMenuItem('copy', 'Copy #${ticket.number}'),
          if (context.read<ConnectionBloc>().state.has(messageLocateFeature)) const GlassMenuItem('link', 'Copy link'),
          const GlassMenuItem('activity', 'Activity'),
          if (ticket.column != ColumnKind.backlog && ticket.column != ColumnKind.done && online) GlassMenuItem('backlog', ticket.column == ColumnKind.planReview ? 'Reject plan' : 'Back to Backlog', danger: true),
          if ((ticket.column == ColumnKind.backlog || ticket.column == ColumnKind.done) && online) GlassMenuItem('archive', 'Archive'),
          // P-23: the main task's own conversation stays reachable.
          if (conversation != null && root?.conversationId != null) const GlassMenuItem('main', 'Open main task conversation'),
        ],
        onSelected: (v) async {
          switch (v) {
            case 'copy':
              await Clipboard.setData(ClipboardData(text: '#${ticket.number} ${ticket.title}'));
              if (context.mounted) toast(context, 'Copied #${ticket.number}.');
            case 'link':
              await copyTicketLink(context, ticket.id, ticket.number);
            case 'activity':
              await showTicketProperties(context, activity: true);
            case 'main':
              await context.push('/conversation/${root!.conversationId}?e=${root.employeeId ?? ''}');
            case 'backlog':
              await backToBacklog(context, ticket);
            case 'archive':
              final done = await ticketWrite(context, ticketArchive(ticketId: ticket.id), expectedVersion: ticket.version, retry: false, done: '#${ticket.number} is archived.');
              if (done && context.mounted && context.canPop()) context.pop();
          }
        },
      );
}

/// The head (TS-05.1): the title, the column chip and "owner X · responsible Y · Project".
class _Head extends StatelessWidget {
  const _Head({required this.ticket, required this.owner, required this.me, required this.quick, this.status, this.mine = false});

  final TicketView ticket;
  final String? owner;
  final String? me;
  final bool quick;

  /// What the ticket waits for, as on its card ("Report to review"); [mine] marks it ◆.
  final String? status;
  final bool mine;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(20, 4, 20, 6),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(ticket.title, key: const ValueKey('ticket-title'), maxLines: 2, overflow: TextOverflow.ellipsis, style: Tokens.title(size: 20)),
          const SizedBox(height: 4),
          Wrap(spacing: 8, runSpacing: 4, crossAxisAlignment: WrapCrossAlignment.center, children: [
            Container(
              key: const ValueKey('ticket-column'),
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
              decoration: BoxDecoration(color: Tokens.cell2, borderRadius: BorderRadius.circular(999)),
              child: Text(columnName(ticket.column), style: const TextStyle(fontWeight: FontWeight.w600)),
            ),
            if (quick) const QuickTag(keyId: 'page'),
            if (status != null) Text(mine ? '◆ $status' : status!, key: const ValueKey('ticket-status'), style: TextStyle(color: mine ? Tokens.accentText : Tokens.label2, fontWeight: mine ? FontWeight.w600 : null)),
          ]),
          const SizedBox(height: 2),
          Text(headLine(ticket, owner: owner, me: me), key: const ValueKey('ticket-head-line'), style: TextStyle(color: Tokens.label2, fontSize: 13)),
          TicketActivityLine(ticketId: ticket.id, padding: const EdgeInsets.only(top: 2)),
        ]),
      );
}

/// The read-only bar of a Done or archived ticket without a conversation (BR5).
class _ReadOnlyBar extends StatelessWidget {
  const _ReadOnlyBar({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) => Container(
        key: const Key('chat-read-only'),
        width: double.infinity,
        decoration: BoxDecoration(color: Tokens.plain, border: Border(top: BorderSide(color: Tokens.sep, width: 0.5))),
        padding: EdgeInsets.fromLTRB(20, 12, 20, 12 + MediaQuery.paddingOf(context).bottom),
        child: Text(text, textAlign: TextAlign.center, style: TextStyle(color: Tokens.label2, fontSize: 13.5)),
      );
}

/// "backend, security ,, ui" → ["backend", "security", "ui"], each once.
List<String> parseLabels(String text) {
  final out = <String>[];
  for (final part in text.split(',')) {
    final label = part.trim();
    if (label.isNotEmpty && !out.contains(label)) out.add(label);
  }
  return out;
}

/// `/ticket/:tid/plan` (§9.1, MU1-13/14): the plan, a field for what should
/// change, and the existing four buttons. The execution mode is the button
/// (D-07); "Request changes" needs feedback (B2); "Reject" puts it back in Backlog.
class TicketPlanPage extends StatelessWidget {
  const TicketPlanPage({super.key, required this.ticketId});

  final String ticketId;

  @override
  Widget build(BuildContext context) => BlocProvider(
        create: (context) => TicketBloc(context.read<AureliusRepository>(), ticketId),
        child: const _PlanView(),
      );
}

class _PlanView extends StatefulWidget {
  const _PlanView();

  @override
  State<_PlanView> createState() => _PlanViewState();
}

class _PlanViewState extends State<_PlanView> {
  final _feedback = TextEditingController();
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    // Kept outside the page, so a sign-out in between does not lose it (R3 M1).
    final repository = context.read<AureliusRepository>();
    final slot = 'plan-feedback:${context.read<TicketBloc>().ticketId}';
    _feedback.text = repository.draft(slot) ?? '';
    _feedback.addListener(() {
      repository.keepDraft(slot, _feedback.text);
      setState(() {});
    });
  }

  @override
  void dispose() {
    _feedback.dispose();
    super.dispose();
  }

  Future<void> _decide(TicketView ticket, Map<String, dynamic> request, String done) async {
    HapticFeedback.mediumImpact();
    setState(() => _busy = true);
    final ok = await ticketWrite(context, request, expectedVersion: ticket.version, retry: false, done: done);
    if (!mounted) return;
    setState(() => _busy = false);
    // Sent: the draft is done with.
    if (ok) _feedback.clear();
    if (ok && context.canPop()) context.pop();
  }

  @override
  Widget build(BuildContext context) {
    final page = context.watch<TicketBloc>().state;
    final online = context.select<ConnectionBloc, bool>((b) => b.state.isOnline);
    final ticket = page.ticket;
    if (ticket == null) {
      return Scaffold(appBar: const GlassAppBar(title: Text('Plan')), body: page.error != null ? ErrorRetry(page.error!) : const Center(child: CircularProgressIndicator()));
    }
    final open = ticket.column == ColumnKind.planReview;
    final can = open && online && !_busy;
    // Phase 2: the feedback is a message of the discussion, not a field here (MU2-05 AC4).
    final discussion = discussionOf(ticket, context.watch<ConnectionBloc>().state) != null;
    final feedback = _feedback.text.trim();
    final plan = page.plan ?? '';
    return Scaffold(
      appBar: GlassAppBar(title: Text('Plan · #${ticket.number}')),
      body: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Padding(padding: const EdgeInsets.fromLTRB(20, 4, 20, 8), child: Text(ticket.title, style: TextStyle(color: Tokens.label2))),
        if (!open)
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
            child: Text('Already handled on another device.', key: const ValueKey('plan-handled'), style: TextStyle(color: Tokens.label2, fontWeight: FontWeight.w600)),
          ),
        Expanded(
          child: plan.trim().isEmpty
              ? Center(child: Text(page.loading ? 'Loading the plan…' : 'This plan has no text.', style: TextStyle(color: Tokens.label2)))
              : SingleChildScrollView(padding: const EdgeInsets.fromLTRB(20, 4, 20, 16), child: MessageMarkdown(key: const ValueKey('ticket-plan-text'), data: plan, scope: null, selectable: true)),
        ),
        if (discussion)
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
            child: OutlinedButton(
              key: const ValueKey('ticket-plan-discuss'),
              onPressed: can ? () => context.canPop() ? context.pop('discussion') : context.pushReplacement('/ticket/${ticket.id}?tab=discussion') : null,
              child: const Text('Request changes in the discussion'),
            ),
          ),
        if (!discussion)
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
          child: Row(children: [
            Expanded(
              child: TextField(
                key: const ValueKey('ticket-plan-feedback'),
                controller: _feedback,
                enabled: can,
                minLines: 1,
                maxLines: 4,
                textCapitalization: TextCapitalization.sentences,
                decoration: const InputDecoration(isDense: true, labelText: 'What needs to change?'),
              ),
            ),
            const SizedBox(width: 8),
            OutlinedButton(
              key: const ValueKey('ticket-plan-revise'),
              onPressed: can && feedback.isNotEmpty ? () => _decide(ticket, ticketRevisePlan(ticketId: ticket.id, feedback: feedback), 'Changes requested. ${ticket.ownerName ?? 'The owner'} makes a new plan.') : null,
              child: const Text('Request changes'),
            ),
          ]),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
          child: Row(children: [
            Expanded(
              child: FilledButton(
                key: const ValueKey('ticket-plan-approve'),
                onPressed: can ? () => _decide(ticket, ticketApprovePlan(ticketId: ticket.id, decision: PlanApproval.approve, feedback: feedback.isEmpty ? null : feedback), 'Approved. The work goes on in Auto.') : null,
                child: const Text('Approve'),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: OutlinedButton(
                key: const ValueKey('ticket-plan-approve-edits'),
                onPressed: can ? () => _decide(ticket, ticketApprovePlan(ticketId: ticket.id, decision: PlanApproval.approveEdits, feedback: feedback.isEmpty ? null : feedback), 'Approved. Edits go through; the rest is asked.') : null,
                child: const Text('Approve, accept edits'),
              ),
            ),
          ]),
        ),
        Padding(
          padding: EdgeInsets.fromLTRB(16, 0, 16, 16 + MediaQuery.paddingOf(context).bottom),
          child: TextButton(
            key: const ValueKey('ticket-plan-reject'),
            style: TextButton.styleFrom(foregroundColor: Tokens.negative),
            onPressed: can
                ? () async {
                    final ok = await backToBacklog(context, ticket);
                    if (ok && context.mounted && context.canPop()) context.pop();
                  }
                : null,
            child: const Text('Reject'),
          ),
        ),
      ]),
    );
  }
}

/// `/ticket/:tid/report`: the main task's report, from the Review card in the
/// discussion ("Report ›"). Decided in the discussion or on the ticket.
class TicketReportPage extends StatelessWidget {
  const TicketReportPage({super.key, required this.ticketId});

  final String ticketId;

  @override
  Widget build(BuildContext context) => BlocProvider(
        create: (context) => TicketBloc(context.read<AureliusRepository>(), ticketId),
        child: Builder(builder: (context) {
          final page = context.watch<TicketBloc>().state;
          final ticket = page.ticket;
          final report = page.root?.result?.trim() ?? '';
          return Scaffold(
            appBar: GlassAppBar(title: Text(ticket == null ? 'Report' : 'Report · #${ticket.number}')),
            body: ticket == null
                ? (page.error != null ? ErrorRetry(page.error!, onRetry: () => context.read<TicketBloc>().add(const TicketRefreshed())) : const Center(child: CircularProgressIndicator()))
                : ListView(padding: EdgeInsets.fromLTRB(20, 8, 20, listBottom(context)), children: [
                    Text('From ${ticket.ownerName ?? 'the owner'}${page.root == null ? '' : ' · ${Fmt.ago(page.root!.updatedAtMs)}'}', style: TextStyle(color: Tokens.label2, fontSize: 13)),
                    const SizedBox(height: 8),
                    report.isEmpty ? Text('The report has no text.', style: TextStyle(color: Tokens.label2)) : MessageMarkdown(key: const ValueKey('ticket-report-page'), data: report, scope: null, selectable: true),
                  ]),
          );
        }),
      );
}
