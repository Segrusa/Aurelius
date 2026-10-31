import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../markdown/links.dart';
import 'ticket_actions.dart';
import 'ticket_bloc.dart';
import 'detail_row.dart';
import 'ticket_card.dart' show priorityColor, startWith;

/// `/ticket/:tid` (§8): a full page with the ticket's fields, its tasks, its
/// activity and, at the bottom, the actions of its column.
class TicketPage extends StatelessWidget {
  const TicketPage({super.key, required this.ticketId});

  final String ticketId;

  @override
  Widget build(BuildContext context) => BlocProvider(
        create: (context) => TicketBloc(context.read<AureliusRepository>(), ticketId),
        child: const _TicketView(),
      );
}

enum _Segment { ticket, activity }

class _TicketView extends StatefulWidget {
  const _TicketView();

  @override
  State<_TicketView> createState() => _TicketViewState();
}

class _TicketViewState extends State<_TicketView> {
  _Segment _segment = _Segment.ticket;

  @override
  Widget build(BuildContext context) {
    final page = context.watch<TicketBloc>().state;
    final company = context.watch<CompanyBloc>().state;
    final online = context.select<ConnectionBloc, bool>((b) => b.state.isOnline);
    final ticket = page.ticket;
    if (ticket == null) {
      return Scaffold(
        appBar: const GlassAppBar(title: Text('Ticket')),
        body: page.error != null
            ? ErrorRetry(page.error!, onRetry: () => context.read<TicketBloc>().add(const TicketRefreshed()))
            : const Center(child: CircularProgressIndicator()),
      );
    }
    final root = page.root;
    final noDispatcher = effectiveDispatcher(company.project(ticket.projectId), company.company?.dispatcherEmployeeId, company.employees) == null;
    final state = ticketState(ticket, root: root, needsOwner: ticket.column == ColumnKind.todo && ticket.ownerEmployeeId == null && (page.gaveUp || noDispatcher), ownerUnknown: page.activityFailed);
    final owner = ticket.ownerName ?? employeeName(company, ticket.ownerEmployeeId);
    final dispatcher = dispatcherName(company, ticket.projectId);
    final line = state.line(owner: owner, dispatcher: dispatcher);
    return Scaffold(
      appBar: GlassAppBar(
        title: Text('#${ticket.number} · ${ticket.projectName}'),
        actions: [
          GlassMenuButton<String>(
            tooltip: 'Ticket actions',
            items: () => [
              GlassMenuItem('copy', 'Copy #${ticket.number}'),
              if (ticket.column != ColumnKind.backlog && ticket.column != ColumnKind.done && online) GlassMenuItem('backlog', ticket.column == ColumnKind.planReview ? 'Afwijzen' : 'Back to Backlog', danger: true),
              if ((ticket.column == ColumnKind.backlog || ticket.column == ColumnKind.done) && online) GlassMenuItem('archive', 'Archive'),
            ],
            onSelected: (v) async {
              switch (v) {
                case 'copy':
                  await Clipboard.setData(ClipboardData(text: '#${ticket.number} ${ticket.title}'));
                  if (context.mounted) toast(context, 'Copied #${ticket.number}.');
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
      bottomNavigationBar: _ActionBar(ticket: ticket, state: state, root: root, online: online),
      body: RefreshIndicator.adaptive(
        onRefresh: () async => context.read<TicketBloc>().add(const TicketRefreshed()),
        child: ListView(
          key: const ValueKey('ticket-page'),
          padding: EdgeInsets.only(bottom: listBottom(context)),
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 8, 20, 4),
              child: Text(ticket.title, key: const ValueKey('ticket-title'), style: Tokens.title(size: 24)),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 4, 20, 8),
              child: Wrap(spacing: 8, runSpacing: 4, crossAxisAlignment: WrapCrossAlignment.center, children: [
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
                  decoration: BoxDecoration(color: Tokens.cell2, borderRadius: BorderRadius.circular(999)),
                  child: Text(_columnName(page, ticket), style: const TextStyle(fontWeight: FontWeight.w600)),
                ),
                if (line != null && state != TicketState.done) Text(state.needsYou ? '◆ $line' : line, style: TextStyle(color: state.needsYou ? Tokens.accentText : Tokens.label2, fontWeight: state.needsYou ? FontWeight.w600 : null)),
              ]),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
              child: PillSegments<_Segment>(
                segments: const [(_Segment.ticket, 'Ticket'), (_Segment.activity, 'Activity')],
                selected: _segment,
                onChanged: (s) => setState(() => _segment = s),
              ),
            ),
            if (_segment == _Segment.ticket) ..._ticketTab(context, page, company, ticket, state, owner, dispatcher, online) else ..._activityTab(page),
          ],
        ),
      ),
    );
  }

  String _columnName(TicketPageState page, TicketView ticket) => switch (ticket.column) {
        ColumnKind.backlog => 'Backlog',
        ColumnKind.todo => 'Todo',
        ColumnKind.doing => 'Doing',
        ColumnKind.planReview => 'Plan review',
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

  List<Widget> _ticketTab(BuildContext context, TicketPageState page, CompanyState company, TicketView ticket, TicketState state, String? owner, String? dispatcher, bool online) {
    final root = page.root;
    final editable = online && ticket.column != ColumnKind.done;
    final running = root != null && (root.status == TaskStatus.running || root.hasLiveRun);
    final tree = ticketTaskTree(page.detail!.tasks);
    final deadline = deadlineOf(ticket);
    final project = company.project(ticket.projectId);
    final inactive = dispatcher == null ? inactiveDispatcher(company, ticket.projectId) : null;
    final ownDispatcher = project?.dispatcherEmployeeId != null && project!.dispatcherEmployeeId == effectiveDispatcher(project, company.company?.dispatcherEmployeeId, company.employees);
    return [
      if (state == TicketState.done)
        _Notice(
          key: const ValueKey('ticket-done'),
          title: '✓ Closed${ticket.doneAtMs == null ? '' : ' · ${DateFormat.yMMMd().add_Hm().format(DateTime.fromMillisecondsSinceEpoch(ticket.doneAtMs!))}'}',
          body: 'Nothing was merged, pushed or deployed.',
        ),
      if (state == TicketState.choosing) _Notice(title: '◌ ${dispatcher ?? 'The dispatcher'} is choosing an employee…', body: 'You can choose one yourself.'),
      if (state == TicketState.needsOwner)
        _Notice(
          key: const ValueKey('ticket-needs-owner'),
          attention: true,
          title: '◆ Needs you · choose an owner',
          body: dispatcher != null
              ? '$dispatcher could not choose an employee.'
              : '${inactive ?? 'No dispatcher is set for this project'}. Started tickets wait in Todo until you choose an owner.',
        ),
      if (state == TicketState.ownerUnknown)
        _Notice(
          key: const ValueKey('ticket-owner-unknown'),
          attention: true,
          title: '◆ Needs you · choose an owner',
          body: 'Could not check whether ${dispatcher ?? 'the dispatcher'} is still choosing. Choose an employee yourself, or try again.',
          action: ('Try again', () => context.read<TicketBloc>().add(const TicketRefreshed())),
        ),
      if (state == TicketState.queued) _Notice(title: 'Queued for ${owner ?? 'its owner'}.'),
      if (state == TicketState.failed)
        _Notice(key: const ValueKey('ticket-failed'), attention: true, title: '✕ The main task failed', body: root?.result ?? 'Start again for a new main task; the old one stays linked.'),
      if (state == TicketState.waitingForYou) _Notice(attention: true, title: '◆ Waiting for your approval', body: 'Answer it in the Inbox.', action: ('Open Inbox', () => context.go('/inbox'))),
      if (state == TicketState.planReview)
        _Notice(attention: true, title: '◆ A plan waits for you', body: 'Approve it, or send it back with what needs to change.', action: ('Review plan', () => context.push('/ticket/${ticket.id}/plan'))),
      if (state == TicketState.review) ...[
        GroupHeader('Report', caps: true, info: 'Only a board member decides. Done merges, pushes and deploys nothing.'),
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
      GroupHeader('Description', caps: true, action: editable ? 'Edit' : null, onAction: () => _editDescription(context, ticket)),
      InsetGroup(padding: const EdgeInsets.all(16), children: [
        (ticket.description ?? '').trim().isEmpty ? Text('No description.', style: TextStyle(color: Tokens.label2)) : MessageMarkdown(data: ticket.description!, scope: null, selectable: true),
      ]),
      GroupHeader('Details', caps: true),
      InsetGroup(children: [
        DetailRow(key: const ValueKey('ticket-row-title'), label: 'Title', value: ticket.title, onTap: editable ? () => _editTitle(context, ticket) : null),
        DetailRow(key: const ValueKey('ticket-row-priority'), label: 'Priority', value: ticket.priority.mark ?? 'Normal', color: priorityColor(ticket.priority), onTap: editable ? () => _editPriority(context, ticket) : null),
        DetailRow(
          key: const ValueKey('ticket-row-owner'),
          label: 'Owner',
          value: owner ?? (ticket.column == ColumnKind.backlog ? 'Dispatcher${dispatcher == null ? '' : ' ($dispatcher)'}' : 'No owner'),
          onTap: online && (ticket.column == ColumnKind.backlog || ticket.column == ColumnKind.todo) ? () => chooseOwner(context, ticket) : null,
        ),
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
        DetailRow(label: 'Created by', value: '${ticket.createdByUsername} · ${DateFormat.MMMd().format(DateTime.fromMillisecondsSinceEpoch(ticket.createdAtMs))}'),
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
      GroupHeader('Discussion', caps: true),
      Padding(padding: const EdgeInsets.fromLTRB(32, 0, 32, 16), child: Text('Arrives in a later version. Until then, use the conversation.', style: TextStyle(color: Tokens.label2))),
    ];
  }

  Future<void> _editTitle(BuildContext context, TicketView ticket) async {
    final values = await promptForm(context, title: 'Title', fields: [FieldSpec('Title', initial: ticket.title, maxLength: 160)]);
    final title = values?.first;
    if (title == null || title.isEmpty || title == ticket.title || !context.mounted) return;
    await ticketWrite(context, ticketUpdate(ticketId: ticket.id, title: title), expectedVersion: ticket.version);
  }

  Future<void> _editDescription(BuildContext context, TicketView ticket) async {
    final values = await promptForm(context, title: 'Description', fields: [FieldSpec('Description', initial: ticket.description ?? '', lines: 6, required: false)]);
    final text = values?.first;
    if (text == null || text == (ticket.description ?? '') || !context.mounted) return;
    // An empty text clears it.
    await ticketWrite(context, ticketUpdate(ticketId: ticket.id, description: text), expectedVersion: ticket.version);
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
  const _ActionBar({required this.ticket, required this.state, required this.root, required this.online});

  final TicketView ticket;
  final TicketState state;
  final TaskView? root;
  final bool online;

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
        TicketState.working || TicketState.waitingForYou => [
            OutlinedButton(key: const ValueKey('bar-backlog'), onPressed: on(() => backToBacklog(context, ticket)), child: const Text('Back to Backlog')),
            if (root?.conversationId case final c?) FilledButton(key: const ValueKey('bar-conversation'), onPressed: () => context.push('/conversation/$c?e=${root!.employeeId ?? ''}'), child: const Text('Conversation')),
          ],
        TicketState.failed => [
            OutlinedButton(key: const ValueKey('bar-backlog'), onPressed: on(() => backToBacklog(context, ticket)), child: const Text('Back to Backlog')),
            FilledButton(key: const ValueKey('bar-start-again'), onPressed: on(() => startTicket(context, ticket)), child: const Text('Start again')),
          ],
        TicketState.planReview => [
            FilledButton(key: const ValueKey('bar-review-plan'), onPressed: () => context.push('/ticket/${ticket.id}/plan'), child: const Text('Review plan')),
          ],
        TicketState.review => [
            OutlinedButton(key: const ValueKey('bar-send-back'), onPressed: on(() => showSendBack(context, ticket)), child: const Text('Send back')),
            FilledButton.icon(key: const ValueKey('bar-done'), onPressed: on(() => showMarkDone(context, ticket)), icon: const Icon(Icons.check), label: const Text('Done')),
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
          child: Row(children: [
            for (final (i, b) in buttons.indexed) ...[if (i > 0) const SizedBox(width: 8), Expanded(child: b)],
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
    _feedback.addListener(() => setState(() {}));
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
