import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/quick_task.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../markdown/links.dart';
import '../my_defaults/run_default_chips.dart';
import 'detail_row.dart';
import 'ticket_actions.dart';
import 'ticket_bloc.dart';
import 'ticket_card.dart' show priorityColor;
import 'ticket_gate.dart' show chooseTicketMode;
import 'ticket_page.dart' show parseLabels, ticketFileRoom, ticketTextUpdate;
import 'ticket_session_rules.dart' show effortChipLabel, modelChipLabel;
import 'ticket_text_dialog.dart';

// The sheet "Ticket properties" of a ticket's session (ticket #71, TS-05.3):
// the ticket's data, its task tree and its Activity, behind the button in the
// head. Every edit is the write it was on the ticket page.

enum _Tab { properties, activity }

/// Opens the properties of the ticket of the [TicketBloc] above [context];
/// [activity] opens on the Activity (an old `?tab=activity`).
Future<void> showTicketProperties(BuildContext context, {bool activity = false}) {
  final bloc = context.read<TicketBloc>();
  return showModalBottomSheet<void>(
    context: context,
    useRootNavigator: true,
    isScrollControlled: true,
    useSafeArea: true,
    showDragHandle: true,
    builder: (_) => BlocProvider.value(value: bloc, child: _PropertiesSheet(activity: activity)),
  );
}

class _PropertiesSheet extends StatefulWidget {
  const _PropertiesSheet({required this.activity});

  final bool activity;

  @override
  State<_PropertiesSheet> createState() => _PropertiesSheetState();
}

class _PropertiesSheetState extends State<_PropertiesSheet> {
  late _Tab _tab = widget.activity ? _Tab.activity : _Tab.properties;

  @override
  Widget build(BuildContext context) {
    final page = context.watch<TicketBloc>().state;
    final ticket = page.ticket;
    return SizedBox(
      height: MediaQuery.sizeOf(context).height * 0.85,
      child: ticket == null
          ? const SizedBox.shrink()
          : Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Padding(padding: const EdgeInsets.fromLTRB(20, 0, 20, 4), child: Text('Ticket properties', style: Tokens.title(size: 20))),
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
                child: PillSegments<_Tab>(
                  key: const ValueKey('ticket-segments'),
                  segments: const [(_Tab.properties, 'Properties'), (_Tab.activity, 'Activity')],
                  selected: _tab,
                  onChanged: (t) => setState(() => _tab = t),
                ),
              ),
              Expanded(
                child: ListView(
                  key: const ValueKey('ticket-page'),
                  padding: EdgeInsets.only(bottom: listBottom(context)),
                  children: _tab == _Tab.properties ? _properties(context, page, ticket) : _activity(page),
                ),
              ),
            ]),
    );
  }

  List<Widget> _activity(TicketPageState page) => [
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

  List<Widget> _properties(BuildContext context, TicketPageState page, TicketView ticket) {
    final company = context.watch<CompanyBloc>().state;
    final link = context.watch<ConnectionBloc>().state;
    final online = link.isOnline;
    final root = page.root;
    final editable = online && ticket.column != ColumnKind.done;
    final running = root != null && (root.status == TaskStatus.running || root.hasLiveRun);
    // The ticket runs on its login user's defaults; null on an older server (the old labels stay).
    final runDefaults = watchRunDefaultLabels(context, loginId: ticket.loginUser, loginName: ticket.loginUsername);
    final tree = ticketTaskTree(page.detail!.tasks);
    final deadline = deadlineOf(ticket);
    final project = company.project(ticket.projectId);
    final owner = ticket.ownerName ?? employeeName(company, ticket.ownerEmployeeId);
    final dispatcher = dispatcherName(company, ticket.projectId);
    final inactive = dispatcher == null ? inactiveDispatcher(company, ticket.projectId) : null;
    final responsibleOn = link.has(ticketResponsibleFeature);
    final followOn = link.has(ticketFollowFeature);
    final quick = isQuickTask(ticket, on: quickTasksOn(link));
    final ownDispatcher = project?.dispatcherEmployeeId != null && project!.dispatcherEmployeeId == effectiveDispatcher(project, company.company?.dispatcherEmployeeId, company.employees);
    return [
      if (ticket.column == ColumnKind.done)
        _Notice(
          key: const ValueKey('ticket-done'),
          title: '✓ Closed${ticket.doneAtMs == null ? '' : ' · ${DateFormat.yMMMd().add_Hm().format(DateTime.fromMillisecondsSinceEpoch(ticket.doneAtMs!))}'}',
          body: !quick
              ? 'Nothing was merged, pushed or deployed.'
              : ticket.mergeCommit == null
                  ? 'There were no commits to merge. Nothing was pushed.'
                  : "Merged into the project's main as ${shortCommit(ticket.mergeCommit!)}. Nothing was pushed.",
        ),
      if (ticket.reviewFeedback != null && ticket.column != ColumnKind.done) _Notice(title: 'Your last feedback', body: ticket.reviewFeedback),
      InsetGroup(children: [
        DetailRow(key: const ValueKey('ticket-row-title'), label: 'Title', value: ticket.title, onTap: editable ? () => _editTitle(context, ticket) : null),
        DetailRow(key: const ValueKey('ticket-row-priority'), label: 'Priority', value: ticket.priority.mark ?? 'Normal', color: priorityColor(ticket.priority), onTap: editable ? () => _editPriority(context, ticket) : null),
        DetailRow(
          key: const ValueKey('ticket-row-owner'),
          label: 'Owner',
          value: owner ?? (ticket.column == ColumnKind.backlog ? 'Dispatcher${dispatcher == null ? '' : ' ($dispatcher)'}' : 'No owner'),
          onTap: online && (ticket.column == ColumnKind.backlog || ticket.column == ColumnKind.todo) ? () => chooseOwner(context, ticket) : null,
        ),
        if (responsibleOn) _responsibleRow(context, ticket, editable),
        DetailRow(
          key: const ValueKey('ticket-row-mode'),
          label: 'Mode',
          value: switch ((ticket.run.mode, page.modeNow)) { (final from?, final now?) => '${from.label} → ${now.label}', (final from, _) => from?.label ?? runDefaults?.mode ?? 'Default' },
          locked: running,
          onTap: !editable
              ? null
              : running
                  ? () => toast(context, 'Locked while the main task runs.')
                  : () => chooseTicketMode(context, ticket),
        ),
        DetailRow(key: const ValueKey('ticket-row-model'), label: 'Model', value: modelChipLabel(ticket.run, runDefaults?.model ?? 'Default'), locked: running),
        if (runDefaults != null) DetailRow(key: const ValueKey('ticket-row-effort'), label: 'Effort', value: effortChipLabel(ticket.run, runDefaults.effort), locked: running),
        DetailRow(key: const ValueKey('ticket-row-labels'), label: 'Labels', value: ticket.labels.isEmpty ? 'None' : ticket.labels.join(', '), onTap: editable ? () => _editLabels(context, ticket) : null),
        DetailRow(key: const ValueKey('ticket-row-deadline'), label: 'Deadline', value: deadline?.text ?? 'None', color: deadline?.late ?? false ? Tokens.negative : null, onTap: editable ? () => _editDeadline(context, ticket) : null),
        DetailRow(key: const ValueKey('ticket-row-project'), label: 'Project', value: ticket.projectName, onTap: () => context.push('/project/${ticket.projectId}')),
        DetailRow(key: const ValueKey('ticket-row-dispatcher'), label: 'Dispatcher', value: dispatcher == null ? 'None${inactive == null ? '' : ' · $inactive'}' : '$dispatcher · ${ownDispatcher ? 'this project' : 'company default'}', onTap: () => context.push('/project/${ticket.projectId}')),
        // MU3-10 AC4: whose subscription it uses, in words; never a choice.
        DetailRow(key: const ValueKey('ticket-row-login'), label: 'Runs on', value: ticket.loginUser == link.user?.id ? 'Your Claude login' : "${ticket.loginUsername}'s Claude login"),
        DetailRow(label: 'Created by', value: '${ticket.createdByUsername} · ${DateFormat.MMMd().format(DateTime.fromMillisecondsSinceEpoch(ticket.createdAtMs))}'),
        if (followOn) ..._followRows(context, page, ticket, online),
        DetailRow(key: const ValueKey('ticket-row-files'), label: 'Files', value: ticket.attachments.isEmpty ? 'None' : ticket.attachments.map((a) => a.name).join(', ')),
      ]),
      GroupHeader('Description', caps: true, action: editable ? 'Edit' : null, onAction: () => _editDescription(context, ticket)),
      InsetGroup(padding: const EdgeInsets.all(16), children: [
        (ticket.description ?? '').trim().isEmpty ? Text('No description.', style: TextStyle(color: Tokens.label2)) : MessageMarkdown(data: ticket.description!, scope: null, selectable: true),
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
              title: t.id == ticket.rootTaskId ? 'Main task · ${t.title}' : t.title,
              subtitle: '${t.employeeName ?? 'Nobody'} · ${t.statusLabel}',
              onTap: () => context.push('/project/${t.projectId}/task/${t.id}'),
            ),
        ]),
      if (root?.conversationId case final conversation?) ...[
        const SizedBox(height: 12),
        InsetGroup(children: [
          GroupRow(key: const ValueKey('ticket-conversation'), leading: const IconTile(CupertinoIcons.chat_bubble_2_fill), title: 'Conversation', subtitle: "The main task's conversation", onTap: () => context.push('/conversation/$conversation?e=${root!.employeeId ?? ''}')),
        ]),
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

class _Notice extends StatelessWidget {
  const _Notice({super.key, required this.title, this.body});

  final String title;
  final String? body;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
        child: Container(
          padding: const EdgeInsets.all(14),
          decoration: BoxDecoration(color: Tokens.cell, borderRadius: BorderRadius.circular(18)),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(title, style: TextStyle(fontWeight: FontWeight.w600, color: Tokens.label)),
            if (body != null) ...[const SizedBox(height: 4), Text(body!, maxLines: 6, overflow: TextOverflow.ellipsis, style: TextStyle(color: Tokens.label2))],
          ]),
        ),
      );
}
