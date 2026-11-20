import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../core/widgets/refresh.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../company_org/employee_widgets.dart' show ProposalCard;
import '../project/task_status_mark.dart';
import 'inbox_bloc.dart';
import 'inbox_rows.dart';
import 'inbox_row_actions.dart';
import 'inbox_sections.dart';
import 'plan_review.dart';

class InboxPage extends StatefulWidget {
  const InboxPage({super.key});

  @override
  State<InboxPage> createState() => _InboxPageState();
}

class _InboxPageState extends State<InboxPage> {
  @override
  void initState() {
    super.initState();
    // Seeing the list counts as reading Done (as on the desktop); the rows of
    // the personal Inbox only by opening them (the bloc keeps them apart).
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) context.read<InboxBloc>().add(const NotificationsMarkedRead());
    });
  }

  @override
  Widget build(BuildContext context) {
    return BlocConsumer<InboxBloc, InboxState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, state) {
        toast(context, state.notice!);
        context.read<InboxBloc>().add(const InboxNoticeShown());
      },
      builder: (context, state) {
        final bloc = context.read<InboxBloc>();
        final company = context.watch<CompanyBloc>().state;
        final s = InboxSections.of(state, company);
        Future<void> refresh() => untilReloaded(bloc, () {
              bloc.add(const InboxRefreshed());
              context.read<CompanyBloc>().add(const CompanyRefreshed());
            }, busy: (s) => s.loading);
        Future<void> clearDone() async {
          final count = s.done;
          if (await confirm(context, title: 'Clear all $count result${count == 1 ? '' : 's'}?', body: 'Approvals are not touched.', action: 'Clear all', danger: true)) {
            bloc.add(const NotificationsCleared());
          }
        }

        // What runs is on the Sessions tab (its badge and Active); the Inbox
        // keeps what needs you and what came back.
        return LargeTitleScaffold(
          title: 'Inbox',
          onRefresh: refresh,
          fill: state.loading && s.isEmpty
              ? const Center(child: CupertinoActivityIndicator())
              : state.loadFailed && s.isEmpty
                  ? EmptyState('Could not load the Inbox.', key: const ValueKey('inbox-load-failed'), action: OutlinedButton(onPressed: () => bloc.add(const InboxRefreshed()), child: const Text('Try again')))
                  : null,
          header: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 0),
              child: Row(children: [
                Expanded(child: _CountTile(key: const ValueKey('inbox-count-needs-you'), count: s.needsYou, label: 'Needs you')),
                const SizedBox(width: 8),
                Expanded(child: _CountTile(key: const ValueKey('inbox-count-updates'), count: s.updates.length, label: 'Updates')),
                const SizedBox(width: 8),
                Expanded(child: _CountTile(key: const ValueKey('inbox-count-done'), count: s.done, label: 'Done')),
              ]),
            ),
          ],
          children: [
            GroupHeader('Needs you', key: const ValueKey('inbox-section-needs-you'), caps: true),
            if (s.needsYou == 0)
              InsetGroup(children: [
                Padding(
                  padding: const EdgeInsets.all(16),
                  child: Row(children: [
                    Icon(CupertinoIcons.checkmark_circle, size: 20, color: Tokens.accent),
                    const SizedBox(width: 12),
                    Expanded(child: Text('Nothing needs you right now.', style: TextStyle(fontSize: 14, color: Tokens.ink2))),
                  ]),
                ),
              ]),
            for (final a in s.approvals) Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 12), child: _ApprovalCard(approval: a)),
            for (final p in s.proposals)
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
                child: ProposalCard(proposal: p, onDecide: (approve, note, instructions) => bloc.add(ProposalDecided(p, approve: approve, note: note, instructions: instructions))),
              ),
            if (s.rowsNeedingYou.isNotEmpty)
              InsetGroup(dividerIndent: 28, children: [for (final n in s.rowsNeedingYou) _swipeAway(bloc, n, _PersonalTile(row: n))]),
            if (s.waitingTasks.isNotEmpty) ...[
              if (s.rowsNeedingYou.isNotEmpty) const SizedBox(height: 12),
              InsetGroup(children: [for (final t in s.waitingTasks) _TaskRow(task: t, company: company)]),
            ],
            GroupHeader(
              state.personalOn ? 'Updates and done' : 'Done',
              key: const ValueKey('inbox-section-done'),
              caps: true,
              action: s.done == 0 ? null : 'Clear all',
              onAction: clearDone,
            ),
            if (s.updates.isEmpty && s.done == 0)
              _EmptyLine(state.personalOn
                  ? "Results land here when an employee finishes. You get one when you're made responsible or @mentioned, and when a ticket you follow needs review, is sent back or fails."
                  : 'Results land here when an employee finishes.'),
            if (s.updates.isNotEmpty)
              InsetGroup(key: const ValueKey('inbox-section-updates'), dividerIndent: 28, children: [for (final n in s.updates) _swipeAway(bloc, n, _PersonalTile(row: n))]),
            if (s.updates.isNotEmpty && s.done > 0) const SizedBox(height: 12),
            if (s.done > 0)
              InsetGroup(
                dividerIndent: 28,
                children: [
                  for (final n in s.results) _swipeAway(bloc, n, _NotificationTile(notification: n, company: company)),
                ],
              ),
          ],
        );
      },
    );
  }
}

/// One of the Inbox's three counts at the top.
class _CountTile extends StatelessWidget {
  const _CountTile({super.key, required this.count, required this.label});

  final int count;
  final String label;

  @override
  Widget build(BuildContext context) => Semantics(
        label: '$label: $count',
        excludeSemantics: true,
        child: Container(
          padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
          decoration: BoxDecoration(
            color: Tokens.cell,
            borderRadius: BorderRadius.circular(14),
            border: Tokens.dark ? null : Border.all(color: Tokens.sep),
          ),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text('$count', style: TextStyle(fontSize: 22, fontWeight: FontWeight.w600, color: Tokens.label, fontFeatures: Tokens.tabular)),
            const SizedBox(height: 2),
            Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 12, color: Tokens.label2)),
          ]),
        ),
      );
}

/// A row that goes with a swipe to the left.
Widget _swipeAway(InboxBloc bloc, NotificationView n, Widget child) => Dismissible(
      key: ValueKey('notification-${n.id}'),
      direction: DismissDirection.endToStart,
      background: Container(
        alignment: Alignment.centerRight,
        padding: const EdgeInsets.only(right: 20),
        color: Tokens.gray,
        child: const Icon(Icons.done_all, color: Colors.white),
      ),
      onDismissed: (_) => bloc.add(NotificationDismissed(n.id)),
      child: child,
    );

/// A row of the personal Inbox (§4.5): what it is about, why, `+n` and the time.
/// A tap goes straight to the place, a long press offers the rest.
class _PersonalTile extends StatelessWidget {
  const _PersonalTile({required this.row});

  final NotificationView row;

  @override
  Widget build(BuildContext context) {
    final n = row;
    final state = context.read<InboxBloc>().state;
    final ticket = n.ticketId;
    return Semantics(
      label: '${n.spoken}, ${Fmt.ago(n.createdAtMs)}',
      button: true,
      excludeSemantics: true,
      child: InkWell(
        key: ValueKey('inbox-row-${n.id}'),
        onTap: () => openInboxRow(context, n),
        onLongPress: () => showInboxRowMenu(context, n, follow: state.followOn, links: state.linksOn),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(12, 12, 16, 12),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              SizedBox(
                width: 14,
                child: n.waitsForMe
                    ? Text('◆', key: ValueKey('inbox-row-diamond-${n.id}'), style: TextStyle(fontSize: 12, color: Tokens.accent))
                    : n.read
                        ? null
                        : Padding(
                            padding: const EdgeInsets.only(top: 5),
                            child: Align(alignment: Alignment.topLeft, child: Container(key: ValueKey('inbox-row-unread-${n.id}'), width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: Tokens.accent))),
                          ),
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (n.place.isNotEmpty) Text(n.place, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 16, height: 1.3, color: Tokens.label, fontWeight: n.read ? FontWeight.w400 : FontWeight.w600)),
                    Padding(
                      padding: const EdgeInsets.only(top: 3),
                      child: Row(
                        children: [
                          Flexible(child: Text(n.headline, maxLines: 2, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 13, color: n.waitsForMe ? Tokens.accentText : Tokens.label2))),
                          if (n.more > 0) ...[
                            const SizedBox(width: 6),
                            // On a ticket it opens the Activity; a channel has none, so the tap is the row's.
                            GestureDetector(
                              key: ValueKey('inbox-row-more-${n.id}'),
                              behavior: HitTestBehavior.opaque,
                              onTap: ticket == null ? null : () => openInboxRowActivity(context, n),
                              child: Padding(padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2), child: Text('+${n.more}', style: TextStyle(fontSize: 13, fontWeight: FontWeight.w600, color: ticket == null ? Tokens.label2 : Tokens.accentText))),
                            ),
                          ],
                        ],
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 8),
              Text(Fmt.ago(n.createdAtMs), style: Tokens.monoSmall()),
            ],
          ),
        ),
      ),
    );
  }
}

class _ApprovalCard extends StatelessWidget {
  const _ApprovalCard({required this.approval});

  final ApprovalView approval;

  @override
  Widget build(BuildContext context) {
    final a = approval;
    void open() => a.isPlan ? showPlanReview(context, a) : context.push('/approval/${a.id}');
    final kind = a.isQuestion ? 'Question' : a.isPlan ? 'Plan' : 'Approval';
    return SurfaceCard(
      onTap: open,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  [kind, if (a.requester.isNotEmpty) a.requester].join(' · '),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 13, color: Tokens.label2),
                ),
              ),
              _Expiry(expiresAtMs: a.expiresAtMs),
            ],
          ),
          const SizedBox(height: 6),
          Text(a.title, style: TextStyle(fontSize: 17, fontWeight: FontWeight.w600, color: Tokens.label)),
          const SizedBox(height: 4),
          Text(
            a.isQuestion ? (a.questions.firstOrNull?.text ?? 'A question') : a.isPlan ? a.planHeadline : a.effectLabel,
            maxLines: 3,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(fontSize: 15, height: 1.4, color: Tokens.label2),
          ),
          const SizedBox(height: 14),
          SizedBox(
            width: double.infinity,
            child: FilledButton(
              key: a.isPlan ? Key('review-plan-${a.id}') : null,
              onPressed: open,
              child: Text(a.isQuestion ? 'Answer' : a.isPlan ? 'Review plan' : 'Review'),
            ),
          ),
        ],
      ),
    );
  }
}

class _Expiry extends StatelessWidget {
  const _Expiry({required this.expiresAtMs});

  final int expiresAtMs;

  @override
  Widget build(BuildContext context) {
    final minutes = ((expiresAtMs - DateTime.now().millisecondsSinceEpoch) / 60000).ceil();
    return Text(minutes <= 0 ? 'expired' : '$minutes min', style: Tokens.monoSmall(color: minutes <= 5 ? Tokens.warning : Tokens.label2));
  }
}

/// The short line under a section header that has nothing in it.
class _EmptyLine extends StatelessWidget {
  const _EmptyLine(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(20, 0, 20, 4),
        child: Text(text, style: TextStyle(fontSize: 14, color: Tokens.label2)),
      );
}

/// A task in Needs you or Running, with its usual status mark; opens the task.
class _TaskRow extends StatelessWidget {
  const _TaskRow({required this.task, required this.company});

  final TaskView task;
  final CompanyState company;

  @override
  Widget build(BuildContext context) {
    final t = task;
    return GroupRow(
      key: ValueKey('inbox-task-${t.id}'),
      onTap: () => context.push('/project/${t.projectId}/task/${t.id}'),
      title: t.title,
      subtitle: [
        taskMarkDetail(t) ?? '',
        company.project(t.projectId)?.name ?? '',
        t.employeeName ?? '',
      ].where((s) => s.isNotEmpty).join(' · '),
      trailing: TaskStatusMark(t),
    );
  }
}

class _NotificationTile extends StatelessWidget {
  const _NotificationTile({required this.notification, required this.company});

  final NotificationView notification;
  final CompanyState company;

  @override
  Widget build(BuildContext context) {
    final n = notification;
    final warning = n.severity == 'warning';
    final task = n.taskId == null ? null : company.tasks.where((t) => t.id == n.taskId).firstOrNull;
    final projectId = task?.projectId;
    return InkWell(
      onTap: projectId == null ? null : () => context.push('/project/$projectId/task/${n.taskId}'),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(12, 12, 16, 12),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(
              width: 8,
              child: Padding(
                padding: const EdgeInsets.only(top: 5),
                child: n.read ? null : Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: warning ? Tokens.warning : Tokens.accent)),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Text(n.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 13, color: warning ? Tokens.warning : Tokens.label2)),
                      ),
                      const SizedBox(width: 8),
                      Text(Fmt.ago(n.createdAtMs), style: Tokens.monoSmall()),
                    ],
                  ),
                  if (n.subject.isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.only(top: 3),
                      child: Text(
                        n.subject,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(fontSize: 16, height: 1.3, color: Tokens.label, fontWeight: n.read ? FontWeight.w400 : FontWeight.w500),
                      ),
                    ),
                  // The task's own mark once it ended (Done, Failed, Cancelled).
                  if (task != null && task.isDone) Padding(padding: const EdgeInsets.only(top: 6), child: TaskStatusMark(task)),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
