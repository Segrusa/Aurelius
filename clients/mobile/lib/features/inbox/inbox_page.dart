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
import 'inbox_sections.dart';
import 'plan_review.dart';
import 'variants_card.dart';

class InboxPage extends StatefulWidget {
  const InboxPage({super.key});

  @override
  State<InboxPage> createState() => _InboxPageState();
}

class _InboxPageState extends State<InboxPage> {
  @override
  void initState() {
    super.initState();
    // Seeing the list counts as reading it (as on the desktop).
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
        final subtitle = [
          if (s.needsYou > 0) '${s.needsYou} need${s.needsYou == 1 ? 's' : ''} you',
          if (s.running > 0) '${s.running} running',
          if (state.unread > 0) '${state.unread} unread',
        ].join(' · ');
        return LargeTitleScaffold(
          title: 'Inbox',
          subtitle: subtitle.isEmpty ? null : subtitle,
          onRefresh: refresh,
          fill: state.loading && s.isEmpty
              ? const Center(child: CupertinoActivityIndicator())
              : s.isEmpty
                  ? const EmptyState('Nothing waiting', detail: 'What needs you, what runs and finished results show up here.')
                  : null,
          children: [
            GroupHeader('Needs you', key: const ValueKey('inbox-section-needs-you'), count: s.needsYou),
            if (s.needsYou == 0) const _EmptyLine('Nothing needs you.'),
            for (final a in s.approvals) Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 12), child: _ApprovalCard(approval: a)),
            for (final p in s.proposals)
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
                child: ProposalCard(proposal: p, onDecide: (approve, note, instructions) => bloc.add(ProposalDecided(p, approve: approve, note: note, instructions: instructions))),
              ),
            for (final t in s.variantTasks)
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
                child: VariantsCard(
                  task: t,
                  variants: state.variants[t.id],
                  subtitle: [company.project(t.projectId)?.name ?? '', t.employeeName ?? ''].where((s) => s.isNotEmpty).join(' · '),
                  choosing: state.deciding.contains('variants:${t.id}'),
                  onLoad: () => bloc.add(VariantsRequested(t.id)),
                  onChoose: (n) => bloc.add(VariantChosen(t.id, variant: n)),
                  onOpen: () => context.push('/project/${t.projectId}/task/${t.id}'),
                ),
              ),
            if (s.waitingTasks.isNotEmpty)
              InsetGroup(children: [for (final t in s.waitingTasks) _TaskRow(task: t, company: company)]),
            GroupHeader('Running', key: const ValueKey('inbox-section-running'), count: s.running),
            if (s.running == 0)
              const _EmptyLine('Nothing running.')
            else
              InsetGroup(
                children: [
                  for (final t in s.runningTasks) _TaskRow(task: t, company: company),
                  for (final a in s.answers) _AnswerRow(answer: a, company: company),
                ],
              ),
            GroupHeader(
              'Done',
              key: const ValueKey('inbox-section-done'),
              count: s.done,
              action: s.done == 0 ? null : 'Clear all',
              onAction: () async {
                final count = s.done;
                if (await confirm(context,
                    title: 'Clear all $count result${count == 1 ? '' : 's'}?',
                    body: 'Approvals are not touched.',
                    action: 'Clear all',
                    danger: true)) {
                  bloc.add(const NotificationsCleared());
                }
              },
            ),
            if (s.done == 0)
              const _EmptyLine('No results yet.')
            else
              InsetGroup(
                dividerIndent: 28,
                children: [
                  for (final n in s.results)
                    Dismissible(
                      key: ValueKey('notification-${n.id}'),
                      direction: DismissDirection.endToStart,
                      background: Container(
                        alignment: Alignment.centerRight,
                        padding: const EdgeInsets.only(right: 20),
                        color: Tokens.gray,
                        child: const Icon(Icons.done_all, color: Colors.white),
                      ),
                      onDismissed: (_) => bloc.add(NotificationDismissed(n.id)),
                      child: _NotificationTile(notification: n, company: company),
                    ),
                ],
              ),
          ],
        );
      },
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
        padding: const EdgeInsets.fromLTRB(32, 0, 24, 4),
        child: Text(text, style: TextStyle(fontSize: 15, color: Tokens.label2)),
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

/// An employee answering in a conversation outside a task; opens it. Its own
/// chat is where it answers the group chat, so that opens Team.
class _AnswerRow extends StatelessWidget {
  const _AnswerRow({required this.answer, required this.company});

  final RunningAnswer answer;
  final CompanyState company;

  @override
  Widget build(BuildContext context) {
    final m = answer.employee;
    final conversationId = answer.agent.conversationId;
    return GroupRow(
      key: ValueKey('inbox-answer-${m.id}-$conversationId'),
      onTap: () => conversationId == null || conversationId == m.conversationId ? context.go('/team') : context.push('/conversation/$conversationId'),
      title: '${m.name} · ${answer.agent.label}',
      subtitle: conversationId == null ? null : m.threadTitle(conversationId),
      trailing: Row(mainAxisSize: MainAxisSize.min, children: [
        const StatusDot(pulse: true, size: 7.5),
        const SizedBox(width: 4),
        Text('Working', style: TextStyle(color: Tokens.text2, fontSize: 12.5, fontWeight: FontWeight.w500)),
      ]),
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
