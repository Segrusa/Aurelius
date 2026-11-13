import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart';
import '../activity/activity_chip.dart';

import '../../core/net/connection.dart' show LinkState;
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
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
import '../team/team_page.dart' show runAuthorOf;
import 'ticket_actions.dart';
import 'ticket_bloc.dart';
import 'ticket_text_dialog.dart';
import 'detail_row.dart';
import 'ticket_card.dart' show QuickTag, WhoBlock, priorityColor, startWith;

/// `/ticket/:tid` (§8): a full page with the ticket's fields, its tasks, its
/// activity and, at the bottom, the actions of its column. With a server of
/// Phase 2 also its discussion (ui-ontwerp Fase 2 §2), where the plan and the
/// report are decided; [discussion] opens on it.
class TicketPage extends StatelessWidget {
  const TicketPage({super.key, required this.ticketId, this.discussion = false, this.activity = false, this.message});

  final String ticketId;
  final bool discussion;

  /// Opens on the Activity: the `+n` of a row in the Inbox.
  final bool activity;

  /// A link to a message of its discussion: scroll to it there.
  final String? message;

  @override
  Widget build(BuildContext context) => BlocProvider(
        // Another ticket on the same route (a `go`) gets its own bloc, not the last one's.
        key: ValueKey('ticket-$ticketId'),
        create: (context) => TicketBloc(context.read<AureliusRepository>(), ticketId),
        child: _TicketView(key: ValueKey('ticket-view-$ticketId'), discussion: discussion, activity: activity, message: message),
      );
}

enum _Segment { ticket, discussion, activity }

/// Run on my login, then the page reads the ticket again at once, not only
/// on its event (QA3 M-m3).
Future<void> _runOnMyLogin(BuildContext context, TicketView ticket) async {
  final bloc = context.read<TicketBloc>();
  if (await runOnMyLogin(context, ticket) && !bloc.isClosed) bloc.add(const TicketRefreshed());
}

/// The discussion of [ticket], when the server has one for it (feature
/// `ticket_discussion` and its `conversation_id`); null: an older server.
String? discussionOf(TicketView ticket, LinkState link) => link.has(ticketDiscussionFeature) ? ticket.conversationId : null;

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

/// How many messages of a discussion this phone showed, by conversation: the
/// Discussion tab gets a dot for newer ones (ui-ontwerp Fase 2 §2). The server
/// keeps no read state for a ticket's discussion.
final _seen = <String, int>{};

class _TicketView extends StatefulWidget {
  const _TicketView({super.key, required this.discussion, this.activity = false, this.message});

  final bool discussion;
  final bool activity;
  final String? message;

  @override
  State<_TicketView> createState() => _TicketViewState();
}

class _TicketViewState extends State<_TicketView> {
  late _Segment _segment = widget.activity ? _Segment.activity : widget.discussion ? _Segment.discussion : _Segment.ticket;

  /// The discussion's composer: Send back and Pas aan take their text from it (MU2-05/06).
  final _composer = TextEditingController();

  @override
  void dispose() {
    _composer.dispose();
    super.dispose();
  }

  /// A decision that needs words (P-27): the Discussion tab, with what to write.
  void _toDiscussion(String why) {
    setState(() => _segment = _Segment.discussion);
    toast(context, why);
  }

  Future<void> _reviewPlan(TicketView ticket) async {
    final next = await context.push<String>('/ticket/${ticket.id}/plan');
    if (next == 'discussion' && mounted) _toDiscussion('Write what needs to change, then Pas aan.');
  }

  @override
  Widget build(BuildContext context) {
    final page = context.watch<TicketBloc>().state;
    final company = context.watch<CompanyBloc>().state;
    final online = context.select<ConnectionBloc, bool>((b) => b.state.isOnline);
    final ticket = page.ticket;
    if (ticket == null) {
      return Scaffold(
        appBar: const GlassAppBar(title: Text('Ticket')),
        body: page.notFound || page.denied
            ? EmptyState(
                page.notFound ? 'Ticket not found' : 'No access',
                key: const ValueKey('ticket-unreachable'),
                detail: page.notFound ? "This ticket doesn't exist on ${context.read<AureliusRepository>().host.name}. It may have been deleted from the Backlog." : "You don't have access to this ticket.",
                action: FilledButton(key: const ValueKey('ticket-go-to-board'), onPressed: () => context.go('/board'), child: const Text('Go to Board')),
              )
            : page.error != null
            ? ErrorRetry(page.error!, onRetry: () => context.read<TicketBloc>().add(const TicketRefreshed()))
            : const Center(child: CircularProgressIndicator()),
      );
    }
    final link = context.watch<ConnectionBloc>().state;
    final conversation = discussionOf(ticket, link);
    final root = page.root;
    final noDispatcher = effectiveDispatcher(company.project(ticket.projectId), company.company?.dispatcherEmployeeId, company.employees) == null;
    final state = ticketState(ticket, root: root, needsOwner: ticket.column == ColumnKind.todo && ticket.ownerEmployeeId == null && (page.gaveUp || noDispatcher), ownerUnknown: page.activityFailed, me: link.user?.id);
    final owner = ticket.ownerName ?? employeeName(company, ticket.ownerEmployeeId);
    final dispatcher = dispatcherName(company, ticket.projectId);
    final expired = link.claudeLogin?.status == ClaudeLoginStatus.expired;
    // The ◆ is the responsible's (B6); the buttons stay for anyone.
    final mine = needsMe(ticket, state, me: link.user?.id);
    final line = state.line(owner: owner, dispatcher: dispatcher, tester: testerName(company), login: ticket.loginUsername, mine: ticket.loginUser == link.user?.id, untilMs: root?.waitingUntilMs, expired: expired, others: decidedByOther(ticket, state, me: link.user?.id));
    final segment = conversation == null && _segment == _Segment.discussion ? _Segment.ticket : _segment;
    final responsibleOn = link.has(ticketResponsibleFeature);
    final followOn = link.has(ticketFollowFeature);
    // Yours to do or not, anyone can decide (B6, §1.4).
    final othersNote = state.needsYou && !mine ? '${responsibleLabel(ticket)} is responsible. You can decide too.' : null;
    final scaffold = Scaffold(
      appBar: GlassAppBar(
        title: Text('#${ticket.number} · ${ticket.projectName}'),
        actions: [
          GlassMenuButton<String>(
            tooltip: 'Ticket actions',
            items: () => [
              GlassMenuItem('copy', 'Copy #${ticket.number}'),
              if (link.has(messageLocateFeature)) const GlassMenuItem('link', 'Copy link'),
              if (ticket.column != ColumnKind.backlog && ticket.column != ColumnKind.done && online) GlassMenuItem('backlog', ticket.column == ColumnKind.planReview ? 'Afwijzen' : 'Back to Backlog', danger: true),
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
                case 'main':
                  await context.push('/conversation/${root!.conversationId}?e=${root.employeeId ?? ''}');
                case 'backlog':
                  await backToBacklog(context, ticket);
                case 'archive':
                  final done = await ticketWrite(context, ticketArchive(ticketId: ticket.id), expectedVersion: ticket.version, retry: false, done: '#${ticket.number} is archived.');
                  if (done && context.mounted && context.canPop()) context.pop();
              }
            },
          ),
        ],
      ),
      // In the discussion its decisions sit under the composer.
      bottomNavigationBar: segment == _Segment.discussion
          ? null
          : _ActionBar(
              ticket: ticket,
              state: state,
              root: root,
              online: online,
              onReviewPlan: () => _reviewPlan(ticket),
              onSendBack: conversation == null ? null : () => _toDiscussion('Write what needs to change, then Send back.'),
              note: othersNote,
            ),
      body: segment == _Segment.discussion
          ? Column(children: [
              if (responsibleOn) Padding(padding: const EdgeInsets.fromLTRB(20, 4, 20, 0), child: WhoBlock(ticket: ticket, owner: owner, me: link.user?.id, keyId: 'strip')),
              _segments(context, conversation),
              Expanded(child: _discussion(context, ticket, state, owner, online, othersNote)),
            ])
          : RefreshIndicator.adaptive(
        onRefresh: () async => context.read<TicketBloc>().add(const TicketRefreshed()),
        child: ListView(
          key: const ValueKey('ticket-page'),
          padding: EdgeInsets.only(bottom: listBottom(context)),
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 8, 20, 4),
              child: Text(ticket.title, key: const ValueKey('ticket-title'), style: Tokens.title(size: 24)),
            ),
            TicketActivityLine(ticketId: ticket.id, padding: const EdgeInsets.fromLTRB(20, 0, 20, 4)),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 4, 20, 8),
              child: Wrap(spacing: 8, runSpacing: 4, crossAxisAlignment: WrapCrossAlignment.center, children: [
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
                  decoration: BoxDecoration(color: Tokens.cell2, borderRadius: BorderRadius.circular(999)),
                  child: Text(_columnName(page, ticket), style: const TextStyle(fontWeight: FontWeight.w600)),
                ),
                if (isQuickTask(ticket, on: quickTasksOn(link))) const QuickTag(keyId: 'page'),
                if (line != null && state != TicketState.done) Text(mine ? '◆ $line' : line, style: TextStyle(color: mine ? Tokens.accentText : Tokens.label2, fontWeight: mine ? FontWeight.w600 : null)),
              ]),
            ),
            // On every tab, so a reviewer sees who does what in the discussion too (P-75).
            if (responsibleOn) Padding(padding: const EdgeInsets.fromLTRB(20, 0, 20, 8), child: WhoBlock(ticket: ticket, owner: owner, me: link.user?.id, keyId: 'strip')),
            _segments(context, conversation),
            if (segment == _Segment.ticket) ..._ticketTab(context, page, company, ticket, state, owner, dispatcher, online, conversation != null, mine: mine, responsibleOn: responsibleOn, followOn: followOn) else ..._activityTab(page),
          ],
        ),
      ),
    );
    if (conversation == null) return scaffold;
    // One bloc for the discussion while the page is open: the tab's dot counts what arrived meanwhile.
    return BlocProvider(
      key: ValueKey('discussion-$conversation'),
      create: (context) => ChatBloc.discussion(context.read<AureliusRepository>(), ticketId: ticket.id, conversationId: conversation, runAuthor: (runId) => runAuthorOf(context.read<CompanyBloc>().state, runId)),
      child: scaffold,
    );
  }

  Widget _segments(BuildContext context, String? conversation) {
    Widget pills(bool unread) => Padding(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
          child: PillSegments<_Segment>(
            key: const ValueKey('ticket-segments'),
            segments: [
              (_Segment.ticket, 'Ticket'),
              if (conversation != null) (_Segment.discussion, unread ? 'Discussion ●' : 'Discussion'),
              (_Segment.activity, 'Activity'),
            ],
            selected: _segment,
            onChanged: (s) => setState(() => _segment = s),
          ),
        );
    if (conversation == null) return pills(false);
    return BlocBuilder<ChatBloc, ChatState>(
      buildWhen: (a, b) => a.messages.length != b.messages.length || a.loading != b.loading,
      builder: (context, chat) {
        if (chat.loading) return pills(false);
        final count = chat.messages.length;
        // First seen on this phone: what is there now counts as read.
        final seen = _seen.putIfAbsent(conversation, () => count);
        if ((_segment == _Segment.discussion || count < seen) && seen != count) _seen[conversation] = count;
        return pills(_segment != _Segment.discussion && count > seen);
      },
    );
  }

  /// The Discussion tab (MU2-01..06): the chat of the ticket, read-only on Done
  /// and when archived (B7).
  Widget _discussion(BuildContext context, TicketView ticket, TicketState state, String? owner, bool online, String? note) {
    final readOnly = discussionReadOnly(ticket);
    final member = context.select<ConnectionBloc, bool>((b) => b.state.user != null);
    return ChatView(
      key: ValueKey('discussion-${ticket.id}'),
      revealMessage: widget.message,
      emptyTitle: 'Discuss the ticket here',
      emptyDetail: 'Employees join when you @mention them or when it starts.',
      composer: readOnly == null,
      readOnlyNote: readOnly,
      hint: discussionHint(ticket, owner: owner),
      source: TicketSource(from: 'Discussion of #${ticket.number}', projectId: ticket.projectId),
      taskProjectId: ticket.projectId,
      composerText: _composer,
      composerActions: !member || readOnly != null || (ticket.column != ColumnKind.planReview && ticket.column != ColumnKind.review)
          ? null
          : (context, text) => _Decisions(ticket: ticket, text: text, online: online, note: note),
    );
  }

  bool _expired(BuildContext context) => context.read<ConnectionBloc>().state.claudeLogin?.status == ClaudeLoginStatus.expired;

  String _columnName(TicketPageState page, TicketView ticket) => switch (ticket.column) {
        ColumnKind.backlog => 'Backlog',
        ColumnKind.todo => 'Todo',
        ColumnKind.doing => 'Doing',
        ColumnKind.planReview => 'Plan review',
        ColumnKind.testing => 'Testing',
        ColumnKind.review => 'Review',
        ColumnKind.done => 'Done',
      };

  List<Widget> _activityTab(TicketPageState page) => [
        if (page.activity.isEmpty) const Padding(padding: EdgeInsets.all(24), child: Text('Nothing happened yet.')),
        if (page.activity.isNotEmpty)
          InsetGroup(children: [
            for (final c in page.activity)
              GroupRow(
                title: activityText(c),
                subtitle: c.detail == 'ticket' ? null : c.detail,
                trailing: Text(Fmt.ago(c.atMs), style: Tokens.monoSmall()),
                titleLines: 2,
              ),
          ]),
      ];

  List<Widget> _ticketTab(BuildContext context, TicketPageState page, CompanyState company, TicketView ticket, TicketState state, String? owner, String? dispatcher, bool online, bool discussion, {required bool mine, required bool responsibleOn, required bool followOn}) {
    final root = page.root;
    final editable = online && ticket.column != ColumnKind.done;
    final running = root != null && (root.status == TaskStatus.running || root.hasLiveRun);
    final tree = ticketTaskTree(page.detail!.tasks);
    final deadline = deadlineOf(ticket);
    final project = company.project(ticket.projectId);
    final inactive = dispatcher == null ? inactiveDispatcher(company, ticket.projectId) : null;
    final ownerRow = DetailRow(
      key: const ValueKey('ticket-row-owner'),
      label: 'Owner',
      value: owner ?? (ticket.column == ColumnKind.backlog ? 'Dispatcher${dispatcher == null ? '' : ' ($dispatcher)'}' : 'No owner'),
      onTap: online && (ticket.column == ColumnKind.backlog || ticket.column == ColumnKind.todo) ? () => chooseOwner(context, ticket) : null,
    );
    final createdRow = DetailRow(label: 'Created by', value: '${ticket.createdByUsername} · ${DateFormat.MMMd().format(DateTime.fromMillisecondsSinceEpoch(ticket.createdAtMs))}');
    // MU3-10 AC4: whose subscription it uses, in words; never a choice.
    final loginRow = DetailRow(
      key: const ValueKey('ticket-row-login'),
      label: 'Runs on',
      value: ticket.loginUser == context.read<ConnectionBloc>().state.user?.id ? 'Your Claude login' : "${ticket.loginUsername}'s Claude login",
    );
    final quick = isQuickTask(ticket, on: quickTasksOn(context.read<ConnectionBloc>().state));
    final ownDispatcher = project?.dispatcherEmployeeId != null && project!.dispatcherEmployeeId == effectiveDispatcher(project, company.company?.dispatcherEmployeeId, company.employees);
    return [
      if (state == TicketState.done)
        _Notice(
          key: const ValueKey('ticket-done'),
          title: '✓ Closed${ticket.doneAtMs == null ? '' : ' · ${DateFormat.yMMMd().add_Hm().format(DateTime.fromMillisecondsSinceEpoch(ticket.doneAtMs!))}'}',
          body: !quick
              ? 'Nothing was merged, pushed or deployed.'
              : ticket.mergeCommit == null
                  ? 'There were no commits to merge. Nothing was pushed.'
                  : "Merged into the project's main as ${shortCommit(ticket.mergeCommit!)}. Nothing was pushed.",
        ),
      if (state == TicketState.choosing) _Notice(title: '◌ ${dispatcher ?? 'The dispatcher'} is choosing an employee…', body: 'You can choose one yourself.'),
      if (state == TicketState.needsOwner)
        _Notice(
          key: const ValueKey('ticket-needs-owner'),
          attention: mine,
          title: mine ? '◆ Needs you · choose an owner' : 'Choose an owner',
          body: dispatcher != null
              ? '$dispatcher could not choose an employee.'
              : '${inactive ?? 'No dispatcher is set for this project'}. Started tickets wait in Todo until you choose an owner.',
        ),
      if (state == TicketState.ownerUnknown)
        _Notice(
          key: const ValueKey('ticket-owner-unknown'),
          attention: mine,
          title: mine ? '◆ Needs you · choose an owner' : 'Choose an owner',
          body: 'Could not check whether ${dispatcher ?? 'the dispatcher'} is still choosing. Choose an employee yourself, or try again.',
          action: ('Try again', () => context.read<TicketBloc>().add(const TicketRefreshed())),
        ),
      if (state == TicketState.queued) _Notice(title: 'Queued for ${owner ?? 'its owner'}.'),
      if (state == TicketState.failed)
        _Notice(key: const ValueKey('ticket-failed'), attention: mine, title: '✕ The main task failed', body: root?.result ?? 'Start again for a new main task; the old one stays linked.'),
      if (state == TicketState.needsYourLogin)
        _Notice(
          key: const ValueKey('ticket-needs-login'),
          attention: true,
          title: '◆ ${state.line(expired: _expired(context))}',
          body: 'It runs only on your own Claude subscription. It goes on by itself once your login is connected.',
          action: (_expired(context) ? 'Reconnect Claude login' : 'Connect Claude login', () => context.push('/account/claude')),
        ),
      if (state == TicketState.waitsForLogin)
        _Notice(
          key: const ValueKey('ticket-waits-login'),
          title: '⏸ ${state.line(login: ticket.loginUsername)}',
          body: "It runs only on ${ticket.loginUsername}'s subscription and goes on once that login is connected. Or run it on your own login.",
          action: online ? ('Run on my login', () => _runOnMyLogin(context, ticket)) : null,
        ),
      if (state == TicketState.waitingForLimit)
        _Notice(
          key: const ValueKey('ticket-limit'),
          title: '⏸ ${state.line(login: ticket.loginUsername, mine: ticket.loginUser == context.read<ConnectionBloc>().state.user?.id, untilMs: root?.waitingUntilMs)}',
          body: "Runs only on the subscription of the login it runs on. It goes on by itself after the reset.",
        ),
      if (state == TicketState.waitingForYou) _Notice(attention: mine, title: mine ? '◆ Waiting for your approval' : 'Waiting for an approval', body: 'Answer it in the Inbox.', action: ('Open Inbox', () => context.go('/inbox'))),
      if (state == TicketState.planReview)
        _Notice(attention: mine, title: mine ? '◆ A plan waits for you' : 'A plan waits for review', body: 'Approve it, or send it back with what needs to change.', action: ('Review plan', () => _reviewPlan(ticket))),
      if (state == TicketState.testing)
        _Notice(
          key: const ValueKey('ticket-testing'),
          title: '${testerName(company) ?? 'The tester'} is testing',
          body: 'The full test suite runs once. Green goes on to Review; red goes back to Doing with the failures for the owner.',
        ),
      if (state == TicketState.review) ...[
        GroupHeader('Report', caps: true, info: quick ? "Only a board member decides. $approveAndMerge merges its branch into the project's main; nothing is pushed." : 'Only a board member decides. Done merges, pushes and deploys nothing.'),
        InsetGroup(padding: const EdgeInsets.all(16), children: [
          Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text('From ${owner ?? 'the owner'}${root == null ? '' : ' · ${Fmt.ago(root.updatedAtMs)}'}', style: TextStyle(color: Tokens.label2, fontSize: 13)),
            const SizedBox(height: 8),
            (root?.result ?? '').trim().isEmpty
                ? Text('The report has no text.', style: TextStyle(color: Tokens.label2))
                : MessageMarkdown(key: const ValueKey('ticket-report'), data: root!.result!, scope: null, selectable: true),
          ]),
        ]),
      ],
      if (ticket.reviewFeedback != null && ticket.column != ColumnKind.done)
        _Notice(title: 'Your last feedback', body: ticket.reviewFeedback),
      if (responsibleOn || followOn) ...[
        GroupHeader('Who', caps: true),
        InsetGroup(children: [
          if (responsibleOn) ...[
            _responsibleRow(context, ticket, editable),
            ownerRow,
            loginRow,
            createdRow,
          ],
          if (followOn) ..._followRows(context, page, ticket, online),
        ]),
      ],
      GroupHeader('Description', caps: true, action: editable ? 'Edit' : null, onAction: () => _editDescription(context, ticket)),
      InsetGroup(padding: const EdgeInsets.all(16), children: [
        (ticket.description ?? '').trim().isEmpty ? Text('No description.', style: TextStyle(color: Tokens.label2)) : MessageMarkdown(data: ticket.description!, scope: null, selectable: true),
      ]),
      GroupHeader('Details', caps: true),
      InsetGroup(children: [
        DetailRow(key: const ValueKey('ticket-row-title'), label: 'Title', value: ticket.title, onTap: editable ? () => _editTitle(context, ticket) : null),
        DetailRow(key: const ValueKey('ticket-row-priority'), label: 'Priority', value: ticket.priority.mark ?? 'Normal', color: priorityColor(ticket.priority), onTap: editable ? () => _editPriority(context, ticket) : null),
        if (!responsibleOn) ownerRow,
        DetailRow(key: const ValueKey('ticket-row-dispatcher'), label: 'Dispatcher', value: dispatcher == null ? 'None${inactive == null ? '' : ' · $inactive'}' : '$dispatcher · ${ownDispatcher ? 'this project' : 'company default'}', onTap: () => context.push('/project/${ticket.projectId}')),
        DetailRow(
          key: const ValueKey('ticket-row-mode'),
          label: 'Mode',
          value: switch ((ticket.run.mode, page.modeNow)) { (final from?, final now?) => '${from.label} → ${now.label}', (final from, _) => from?.label ?? 'Default' },
          locked: running,
          onTap: !editable
              ? null
              : running
                  ? () => toast(context, 'Locked while the main task runs.')
                  : () => _editMode(context, ticket),
        ),
        DetailRow(label: 'Model', value: ticket.run.model ?? 'Default', locked: running),
        DetailRow(key: const ValueKey('ticket-row-labels'), label: 'Labels', value: ticket.labels.isEmpty ? 'None' : ticket.labels.join(', '), onTap: editable ? () => _editLabels(context, ticket) : null),
        DetailRow(key: const ValueKey('ticket-row-deadline'), label: 'Deadline', value: deadline?.text ?? 'None', color: deadline?.late ?? false ? Tokens.negative : null, onTap: editable ? () => _editDeadline(context, ticket) : null),
        DetailRow(label: 'Attachments', value: ticket.attachments.isEmpty ? 'None' : '${ticket.attachments.length} file${ticket.attachments.length == 1 ? '' : 's'}'),
        if (!responsibleOn) ...[createdRow, loginRow],
      ]),
      GroupHeader('Tasks', caps: true, count: tree.length),
      if (tree.isEmpty)
        Padding(padding: const EdgeInsets.fromLTRB(32, 0, 32, 0), child: Text('The main task starts when the ticket starts.', style: TextStyle(color: Tokens.label2)))
      else
        InsetGroup(children: [
          for (final (t, depth) in tree)
            GroupRow(
              key: ValueKey('ticket-task-${t.id}'),
              padding: EdgeInsets.fromLTRB(16.0 + depth * 18, 12, 16, 12),
              title: t.title,
              subtitle: '${t.employeeName ?? 'Nobody'} · ${t.statusLabel}',
              onTap: () => context.push('/project/${t.projectId}/task/${t.id}'),
            ),
        ]),
      if (root?.conversationId case final conversation?) ...[
        const SizedBox(height: 12),
        InsetGroup(children: [
          GroupRow(key: const ValueKey('ticket-conversation'), leading: const IconTile(CupertinoIcons.chat_bubble_2_fill), title: 'Conversation', subtitle: 'The main task\'s conversation', onTap: () => context.push('/conversation/$conversation?e=${root!.employeeId ?? ''}')),
        ]),
      ],
      if (!discussion) ...[
        GroupHeader('Discussion', caps: true),
        Padding(padding: const EdgeInsets.fromLTRB(32, 0, 32, 16), child: Text('Arrives in a later version. Until then, use the conversation.', style: TextStyle(color: Tokens.label2))),
      ],
    ];
  }

  /// Followers and the switch Following (§3.2): the responsible's stays on and
  /// off limits (B10). A failed read says so; the switch waits for a good one.
  List<Widget> _followRows(BuildContext context, TicketPageState page, TicketView ticket, bool online) {
    final followers = page.followers;
    final me = context.read<ConnectionBloc>().state.user?.id;
    final mine = followers?.where((f) => f.userId == me).firstOrNull;
    final following = mine != null;
    final known = followers != null;
    final bloc = context.read<TicketBloc>();
    Future<void> set(bool on) async {
      if (await followTicket(context, ticket, on: on) && !bloc.isClosed) bloc.add(const TicketRefreshed());
    }

    return [
      DetailRow(
        key: const ValueKey('ticket-row-followers'),
        label: 'Followers',
        value: page.followersFailed && !known ? 'Could not load · Try again' : known ? '${followers.length}' : '',
        onTap: known ? () => showFollowers(context, ticket, followers) : () => bloc.add(const TicketRefreshed()),
      ),
      SwitchListTile.adaptive(
        key: const ValueKey('ticket-following'),
        title: const Text('Following'),
        subtitle: Text(followingNote(ticket, reason: mine?.reason, me: me), key: const ValueKey('ticket-following-note'), style: TextStyle(color: Tokens.label2, fontSize: 13)),
        value: following || !canUnfollow(ticket, me: me),
        onChanged: online && known && canUnfollow(ticket, me: me) ? set : null,
      ),
    ];
  }

  /// The field Responsible (§1.4, §2.2): its picker, the disabled one's note and read-only on Done.
  Widget _responsibleRow(BuildContext context, TicketView ticket, bool editable) {
    final me = context.read<ConnectionBloc>().state.user?.id;
    final disabled = ticket.responsibleDisabled ?? false;
    final name = responsibleLabel(ticket, me: me);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      DetailRow(key: const ValueKey('ticket-row-responsible'), label: 'Responsible', value: name ?? 'No one', onTap: editable ? () => chooseResponsible(context, ticket) : null),
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 10),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(
            disabled ? '${ticket.responsibleUsername} is disabled and gets no updates. Choose someone else.' : ticket.column == ColumnKind.done ? 'Done tickets are closed.' : responsibleExplained,
            key: const ValueKey('ticket-responsible-note'),
            style: TextStyle(color: Tokens.label2, fontSize: 13),
          ),
          if (editable && name == null)
            TextButton(key: const ValueKey('ticket-make-me-responsible'), style: TextButton.styleFrom(padding: EdgeInsets.zero), onPressed: () => setResponsible(context, ticket, me), child: const Text('Make me responsible')),
        ]),
      ),
    ]);
  }

  Future<void> _editTitle(BuildContext context, TicketView ticket) async {
    final room = ticketFileRoom(ticket, context.read<ConnectionBloc>().state);
    final edit = await editTicketText(context, label: 'Title', initial: ticket.title, maxLength: 160, room: room);
    final request = edit == null ? null : ticketTextUpdate(ticket, edit, title: true);
    if (request == null || !context.mounted) return;
    await ticketWrite(context, request, expectedVersion: ticket.version);
  }

  Future<void> _editDescription(BuildContext context, TicketView ticket) async {
    final room = ticketFileRoom(ticket, context.read<ConnectionBloc>().state);
    final edit = await editTicketText(context, label: 'Description', initial: ticket.description ?? '', lines: 6, required: false, room: room);
    final request = edit == null ? null : ticketTextUpdate(ticket, edit, title: false);
    if (request == null || !context.mounted) return;
    await ticketWrite(context, request, expectedVersion: ticket.version);
  }

  Future<void> _editPriority(BuildContext context, TicketView ticket) async {
    final picked = await showModalBottomSheet<TicketPriority>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      builder: (sheet) => SafeArea(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          for (final p in ticketPriorities.reversed)
            ListTile(
              key: ValueKey('priority-${p.wire}'),
              title: Text(p.mark ?? 'Normal', style: TextStyle(color: priorityColor(p))),
              trailing: p == ticket.priority ? Icon(Icons.check, color: Tokens.accentText) : null,
              onTap: () => Navigator.pop(sheet, p),
            ),
        ]),
      ),
    );
    if (picked == null || picked == ticket.priority || !context.mounted) return;
    await ticketWrite(context, ticketUpdate(ticketId: ticket.id, priority: picked), expectedVersion: ticket.version);
  }

  Future<void> _editMode(BuildContext context, TicketView ticket) async {
    final picked = await showModalBottomSheet<(AgentMode?,)>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      builder: (sheet) => SafeArea(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          ListTile(title: const Text('Default'), subtitle: const Text("The owner's own mode"), trailing: ticket.run.mode == null ? Icon(Icons.check, color: Tokens.accentText) : null, onTap: () => Navigator.pop(sheet, (null,))),
          for (final m in agentModes)
            ListTile(
              key: ValueKey('mode-${m.wire}'),
              title: Text(m.label),
              subtitle: m == AgentMode.plan ? const Text('Stops in Plan review with a plan before anything changes.') : null,
              trailing: ticket.run.mode == m ? Icon(Icons.check, color: Tokens.accentText) : null,
              onTap: () => Navigator.pop(sheet, (m,)),
            ),
        ]),
      ),
    );
    if (picked == null || picked.$1 == ticket.run.mode || !context.mounted) return;
    await ticketWrite(context, ticketUpdate(ticketId: ticket.id, run: TaskRun(mode: picked.$1, model: ticket.run.model, effort: ticket.run.effort)), expectedVersion: ticket.version);
  }

  Future<void> _editLabels(BuildContext context, TicketView ticket) async {
    final values = await promptForm(context, title: 'Labels', fields: [FieldSpec('Labels', initial: ticket.labels.join(', '), hint: 'backend, security', required: false)]);
    if (values == null || !context.mounted) return;
    final labels = parseLabels(values.first);
    if (labels.join(',') == ticket.labels.join(',')) return;
    await ticketWrite(context, ticketUpdate(ticketId: ticket.id, labels: labels), expectedVersion: ticket.version);
  }

  Future<void> _editDeadline(BuildContext context, TicketView ticket) async {
    final now = DateTime.now();
    final current = ticket.deadlineMs == null ? null : DateTime.fromMillisecondsSinceEpoch(ticket.deadlineMs!);
    if (current != null) {
      final clear = await showModalBottomSheet<bool>(
        context: context,
        useRootNavigator: true,
        showDragHandle: true,
        builder: (sheet) => SafeArea(
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            ListTile(title: const Text('Change the date'), onTap: () => Navigator.pop(sheet, false)),
            ListTile(key: const ValueKey('deadline-clear'), title: Text('Clear', style: TextStyle(color: Tokens.negative)), onTap: () => Navigator.pop(sheet, true)),
          ]),
        ),
      );
      if (clear == null || !context.mounted) return;
      if (clear) {
        await ticketWrite(context, ticketUpdate(ticketId: ticket.id, clearDeadline: true), expectedVersion: ticket.version);
        return;
      }
    }
    final day = await showDatePicker(context: context, initialDate: current ?? now, firstDate: DateTime(now.year - 1), lastDate: DateTime(now.year + 10));
    if (day == null || !context.mounted) return;
    final at = DateTime(day.year, day.month, day.day, 17);
    await ticketWrite(context, ticketUpdate(ticketId: ticket.id, deadlineMs: at.millisecondsSinceEpoch), expectedVersion: ticket.version);
  }
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

class _Notice extends StatelessWidget {
  const _Notice({super.key, required this.title, this.body, this.attention = false, this.action});

  final String title;
  final String? body;
  final bool attention;
  final (String, VoidCallback)? action;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
        child: Container(
          padding: const EdgeInsets.all(14),
          decoration: BoxDecoration(
            color: Tokens.cell,
            borderRadius: BorderRadius.circular(18),
            border: attention ? Border.all(color: Tokens.accent.withValues(alpha: 0.45)) : null,
          ),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(title, style: TextStyle(fontWeight: FontWeight.w600, color: attention ? Tokens.accentText : Tokens.label)),
            if (body != null) ...[const SizedBox(height: 4), Text(body!, maxLines: 6, overflow: TextOverflow.ellipsis, style: TextStyle(color: Tokens.label2))],
            if (action != null) ...[const SizedBox(height: 8), OutlinedButton(onPressed: action!.$2, child: Text(action!.$1))],
          ]),
        ),
      );
}

/// The glass bar at the bottom (§8): at most two buttons, the main one right.
class _ActionBar extends StatelessWidget {
  const _ActionBar({required this.ticket, required this.state, required this.root, required this.online, required this.onReviewPlan, this.onSendBack, this.note});

  final TicketView ticket;
  final TicketState state;
  final TaskView? root;
  final bool online;
  final VoidCallback onReviewPlan;

  /// With a discussion: Send back goes to its composer (P-27); without, the Fase 1 sheet.
  final VoidCallback? onSendBack;

  /// Above the buttons when the decision is someone else's: "Bo is responsible. You can decide too."
  final String? note;

  @override
  Widget build(BuildContext context) {
    VoidCallback? on(VoidCallback f) => online ? f : null;
    final buttons = <Widget>[
      ...switch (state) {
        TicketState.backlog => [
            OutlinedButton(key: const ValueKey('bar-owner'), onPressed: on(() => chooseOwner(context, ticket)), child: const Text('Owner…')),
            OutlinedButton(key: const ValueKey('bar-start-with'), onPressed: on(() => startWith(context, ticket)), child: const Text('Start with…')),
            FilledButton(key: const ValueKey('bar-start'), onPressed: on(() => startTicket(context, ticket)), child: const Text('▸ Start')),
          ],
        TicketState.choosing || TicketState.needsOwner || TicketState.ownerUnknown || TicketState.queued => [
            OutlinedButton(key: const ValueKey('bar-backlog'), onPressed: on(() => backToBacklog(context, ticket)), child: const Text('Back to Backlog')),
            if (state != TicketState.queued) FilledButton(key: const ValueKey('bar-choose'), onPressed: on(() => chooseOwner(context, ticket)), child: const Text('Choose employee')),
          ],
        TicketState.needsYourLogin => [
            OutlinedButton(key: const ValueKey('bar-backlog'), onPressed: on(() => backToBacklog(context, ticket)), child: const Text('Back to Backlog')),
            FilledButton(
              key: const ValueKey('bar-connect'),
              onPressed: () => context.push('/account/claude'),
              child: Text(context.read<ConnectionBloc>().state.claudeLogin?.status == ClaudeLoginStatus.expired ? 'Reconnect Claude login' : 'Connect Claude login'),
            ),
          ],
        TicketState.waitsForLogin => [
            OutlinedButton(key: const ValueKey('bar-backlog'), onPressed: on(() => backToBacklog(context, ticket)), child: const Text('Back to Backlog')),
            FilledButton(key: const ValueKey('bar-use-my-login'), onPressed: on(() => _runOnMyLogin(context, ticket)), child: const Text('Run on my login')),
          ],
        // The tester moves it on (ticket #5): only a cancel, no Done or Send back.
        TicketState.waitingForLimit || TicketState.testing => [
            OutlinedButton(key: const ValueKey('bar-backlog'), onPressed: on(() => backToBacklog(context, ticket)), child: const Text('Back to Backlog')),
          ],
        TicketState.working || TicketState.waitingForYou => [
            OutlinedButton(key: const ValueKey('bar-backlog'), onPressed: on(() => backToBacklog(context, ticket)), child: const Text('Back to Backlog')),
            if (root?.conversationId case final c?) FilledButton(key: const ValueKey('bar-conversation'), onPressed: () => context.push('/conversation/$c?e=${root!.employeeId ?? ''}'), child: const Text('Conversation')),
          ],
        TicketState.failed => [
            OutlinedButton(key: const ValueKey('bar-backlog'), onPressed: on(() => backToBacklog(context, ticket)), child: const Text('Back to Backlog')),
            FilledButton(key: const ValueKey('bar-start-again'), onPressed: on(() => startTicket(context, ticket)), child: const Text('Start again')),
          ],
        TicketState.planReview => [
            FilledButton(key: const ValueKey('bar-review-plan'), onPressed: onReviewPlan, child: const Text('Review plan')),
          ],
        TicketState.review => [
            OutlinedButton(key: const ValueKey('bar-send-back'), onPressed: on(onSendBack ?? () => showSendBack(context, ticket)), child: const Text('Send back')),
            FilledButton.icon(key: const ValueKey('bar-done'), onPressed: on(() => showMarkDone(context, ticket)), icon: const Icon(Icons.check), label: Text(acceptLabel(ticket, quickOn: quickTasksOn(context.read<ConnectionBloc>().state)))),
          ],
        TicketState.done => const <Widget>[],
      },
    ];
    if (buttons.isEmpty) return const SizedBox.shrink();
    return Glass(
      radius: 0,
      flat: true,
      child: SafeArea(
        top: false,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(16, 10, 16, 10),
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            if (note != null) Padding(padding: const EdgeInsets.only(bottom: 8), child: Text(note!, key: const ValueKey('bar-others-note'), style: TextStyle(color: Tokens.label2, fontSize: 13))),
            Row(children: [
              for (final (i, b) in buttons.indexed) ...[if (i > 0) const SizedBox(width: 8), Expanded(child: b)],
            ]),
          ]),
        ),
      ),
    );
  }
}

/// `/ticket/:tid/plan` (§9.1, MU1-13/14): the plan, a field for what should
/// change, and the existing four buttons. The execution mode is the button
/// (D-07); "Pas aan" needs feedback (B2); "Afwijzen" puts it back in Backlog.
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
                decoration: const InputDecoration(isDense: true, labelText: 'Wat moet er anders?'),
              ),
            ),
            const SizedBox(width: 8),
            OutlinedButton(
              key: const ValueKey('ticket-plan-revise'),
              onPressed: can && feedback.isNotEmpty ? () => _decide(ticket, ticketRevisePlan(ticketId: ticket.id, feedback: feedback), 'Changes requested. ${ticket.ownerName ?? 'The owner'} makes a new plan.') : null,
              child: const Text('Pas aan'),
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
                child: const Text('Goedkeuren + Auto'),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: OutlinedButton(
                key: const ValueKey('ticket-plan-approve-edits'),
                onPressed: can ? () => _decide(ticket, ticketApprovePlan(ticketId: ticket.id, decision: PlanApproval.approveEdits, feedback: feedback.isEmpty ? null : feedback), 'Approved. Edits go through; the rest is asked.') : null,
                child: const Text('Goedkeuren'),
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
            child: const Text('Afwijzen'),
          ),
        ),
      ]),
    );
  }
}

/// Under the discussion's composer (ui-ontwerp Fase 2 §1, MU2-05/06): in Plan
/// review Goedkeuren + Auto · Goedkeuren · Pas aan · Afwijzen, in Review Send
/// back and ✓ Done. Pas aan and Send back take the composer's text as the
/// feedback (the server puts it in the discussion) and stay off without it.
class _Decisions extends StatefulWidget {
  const _Decisions({required this.ticket, required this.text, required this.online, this.note});

  final TicketView ticket;
  final TextEditingController text;
  final bool online;

  /// Above the buttons when the decision is someone else's (§1.4).
  final String? note;

  @override
  State<_Decisions> createState() => _DecisionsState();
}

class _DecisionsState extends State<_Decisions> {
  bool _busy = false;

  Future<void> _decide(Map<String, dynamic> request, String done) async {
    HapticFeedback.mediumImpact();
    setState(() => _busy = true);
    final ok = await ticketWrite(context, request, expectedVersion: widget.ticket.version, retry: false, done: done);
    if (!mounted) return;
    setState(() => _busy = false);
    if (ok) widget.text.clear();
  }

  @override
  Widget build(BuildContext context) {
    final ticket = widget.ticket;
    final owner = ticket.ownerName ?? 'The owner';
    return ValueListenableBuilder<TextEditingValue>(
      valueListenable: widget.text,
      builder: (context, value, _) {
        final can = widget.online && !_busy;
        final words = value.text.trim();
        final feedback = words.isEmpty ? null : words;
        final buttons = ticket.column == ColumnKind.planReview
            ? <Widget>[
                FilledButton(
                  key: const ValueKey('discussion-approve-auto'),
                  onPressed: can ? () => _decide(ticketApprovePlan(ticketId: ticket.id, decision: PlanApproval.approve, feedback: feedback), 'Approved. The work goes on in Auto.') : null,
                  child: const Text('Goedkeuren + Auto'),
                ),
                OutlinedButton(
                  key: const ValueKey('discussion-approve'),
                  onPressed: can ? () => _decide(ticketApprovePlan(ticketId: ticket.id, decision: PlanApproval.approveEdits, feedback: feedback), 'Approved. Edits go through; the rest is asked.') : null,
                  child: const Text('Goedkeuren'),
                ),
                OutlinedButton(
                  key: const ValueKey('discussion-revise'),
                  onPressed: can && feedback != null ? () => _decide(ticketRevisePlan(ticketId: ticket.id, feedback: feedback), 'Changes requested. $owner makes a new plan.') : null,
                  child: const Text('Pas aan'),
                ),
                TextButton(
                  key: const ValueKey('discussion-reject'),
                  style: TextButton.styleFrom(foregroundColor: Tokens.negative),
                  onPressed: can ? () => backToBacklog(context, ticket) : null,
                  child: const Text('Afwijzen'),
                ),
              ]
            : <Widget>[
                OutlinedButton(
                  key: const ValueKey('discussion-send-back'),
                  onPressed: !can
                      ? null
                      : feedback == null
                          ? () => toast(context, 'Write what needs to change first.')
                          : () => _decide(ticketSendBack(ticketId: ticket.id, feedback: feedback), '#${ticket.number} is back in Doing with your feedback.'),
                  child: const Text('Send back to Doing'),
                ),
                FilledButton.icon(
                  key: const ValueKey('discussion-done'),
                  onPressed: can ? () => showMarkDone(context, ticket) : null,
                  icon: const Icon(Icons.check, size: 18),
                  label: Text(acceptLabel(ticket, quickOn: quickTasksOn(context.read<ConnectionBloc>().state))),
                ),
              ];
        return Padding(
          key: const ValueKey('discussion-decisions'),
          padding: const EdgeInsets.only(top: 8),
          // All of them in sight on a phone: they wrap rather than scroll.
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            if (widget.note != null) Padding(padding: const EdgeInsets.only(bottom: 6), child: Text(widget.note!, style: TextStyle(color: Tokens.label2, fontSize: 13))),
            Wrap(spacing: 6, runSpacing: 6, crossAxisAlignment: WrapCrossAlignment.center, children: buttons),
          ]),
        );
      },
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
