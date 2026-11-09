import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/widgets.dart';
import '../../core/widgets/refresh.dart';
import '../../data/aurelius_repository.dart';
import '../../data/company.dart';
import '../../data/views.dart';
import '../../data/run_config.dart';
import '../markdown/links.dart';
import '../chat/upload_files.dart';
import '../company/company_bloc.dart';
import '../connection/connection_banner.dart';
import '../inbox/inbox_bloc.dart';
import 'new_task_page.dart' show taskModes;
import 'project_bloc.dart';
import 'task_status_mark.dart';
import '../../core/widgets/ios.dart';

/// A file attached to the task: a chip, with a thumbnail for an image; tap opens
/// it when the server can send it (older servers: the chip only).
class _Attachment extends StatelessWidget {
  const _Attachment({required this.file});

  final MessageFileView file;

  @override
  Widget build(BuildContext context) {
    final image = isImageName(file.name);
    final chip = Chip(
      avatar: Icon(image ? Icons.image_outlined : Icons.description_outlined, size: 18, color: Tokens.accentText),
      label: Text('${file.name} · ${Fmt.bytes(file.byteLength)}', overflow: TextOverflow.ellipsis),
    );
    if (!uploadFetchable(context, file)) return chip;
    return GestureDetector(
      key: Key('attachment-${file.id}'),
      onTap: () => openUpload(context, file),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (image)
            Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: UploadThumb(id: file.id!, width: 120, height: 80, fallback: const SizedBox.shrink()),
            ),
          AbsorbPointer(child: chip),
        ],
      ),
    );
  }
}

class TaskPage extends StatelessWidget {
  const TaskPage({super.key, required this.projectId, required this.taskId});

  final String projectId;
  final String taskId;

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (context) => ProjectBloc(context.read<AureliusRepository>(), projectId),
      child: BlocConsumer<ProjectBloc, ProjectState>(
        listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
        listener: (context, state) {
          if (state.notice != null) toast(context, state.notice!);
          context.read<ProjectBloc>().add(const ProjectNoticeShown());
        },
        builder: (context, state) {
          final task = state.task(taskId);
          return Scaffold(
            appBar: GlassAppBar(
              title: Text(state.project?.name ?? 'Task', style: Tokens.title(size: 17)),
            ),
            body: Column(
              children: [
                _ReadOnOpen(taskId: taskId),
                const ConnectionBanner(),
                Expanded(
                  child: task == null
                      ? (state.loading ? const Center(child: CircularProgressIndicator()) : const EmptyState('This task no longer exists'))
                      : _TaskBody(task: task, busy: state.busy, tasks: state.tasks),
                ),
              ],
            ),
          );
        },
      ),
    );
  }
}

/// Opening the task reads its result notifications (see `TaskOpened`).
class _ReadOnOpen extends StatefulWidget {
  const _ReadOnOpen({required this.taskId});

  final String taskId;

  @override
  State<_ReadOnOpen> createState() => _ReadOnOpenState();
}

class _ReadOnOpenState extends State<_ReadOnOpen> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      try {
        context.read<InboxBloc>().add(TaskOpened(widget.taskId));
      } on Object {
        // No inbox here (previews, tests).
      }
    });
  }

  @override
  Widget build(BuildContext context) => const SizedBox.shrink();
}

class _TaskBody extends StatelessWidget {
  const _TaskBody({required this.task, required this.busy, this.tasks = const []});

  final TaskView task;
  final bool busy;
  final List<TaskView> tasks;

  /// Give the task to an active employee (the management team first, then
  /// the others), or make it a draft.
  Future<void> _assign(BuildContext context) async {
    final bloc = context.read<ProjectBloc>();
    final company = context.read<CompanyBloc>().state;
    final groups = pickerGroups(company.employees.where((m) => m.status == LifecycleStatus.active).toList());
    final choice = await showModalBottomSheet<(String?, String?)>(
      context: context,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.75),
          child: ListView(
            shrinkWrap: true,
            children: [
              const Padding(padding: EdgeInsets.fromLTRB(16, 16, 16, 0), child: Eyebrow('Give the task to')),
              for (final g in groups) ...[
                Padding(padding: const EdgeInsets.fromLTRB(16, 12, 16, 4), child: Text(g.title, style: TextStyle(fontSize: 13, fontWeight: FontWeight.w600, color: Tokens.muted))),
                for (final m in g.employees)
                  ListTile(
                    key: ValueKey('assign-${m.id}'),
                    leading: CircleAvatar(child: Text(m.monogram)),
                    title: Text(m.name),
                    subtitle: m.roleLine.isEmpty ? null : Text(m.roleLine),
                    trailing: task.employeeId == m.id ? Icon(Icons.check, color: Tokens.accent) : null,
                    onTap: () => Navigator.pop(sheet, (m.id, m.name)),
                  ),
              ],
              ListTile(
                leading: const Icon(Icons.edit_note),
                title: const Text('Nobody (draft)'),
                subtitle: const Text('Nothing runs until someone leads it'),
                trailing: task.employeeId == null ? Icon(Icons.check, color: Tokens.accent) : null,
                onTap: () => Navigator.pop(sheet, (null, null)),
              ),
            ],
          ),
        ),
      ),
    );
    if (choice != null && choice.$1 != task.employeeId) bloc.assignTask(task.id, choice.$1, choice.$2);
  }

  /// "Iris · Head of marketing", the lead employee with its title.
  String _leader(BuildContext context) {
    final line = context.read<CompanyBloc>().state.employee(task.employeeId ?? '')?.roleLine ?? '';
    return line.isEmpty ? task.employeeName! : '${task.employeeName} · $line';
  }

  String? _parentTitle() => tasks.where((t) => t.id == task.parentTaskId).map((t) => t.title).firstOrNull;

  @override
  Widget build(BuildContext context) {
    final bloc = context.read<ProjectBloc>();
    return PullToRefresh(
      scrollable: true,
      onRefresh: () => untilReloaded(bloc, () => bloc.add(const ProjectRefreshed()), busy: (s) => s.loading),
      child: ListView(
      padding: EdgeInsets.fromLTRB(16, 8, 16, listBottom(context)),
      children: [
        Row(children: [
          TaskStatusMark(task, size: 14),
          if (taskMarkDetail(task) case final detail?)
            Flexible(child: Text('  ${detail.toUpperCase()}', maxLines: 1, overflow: TextOverflow.ellipsis, style: Tokens.eyebrow().copyWith(color: task.needsYou ? Tokens.accentText : Tokens.muted))),
        ]),
        const SizedBox(height: 6),
        Text(task.title, style: Tokens.title(size: 26)),
        if (task.employeeName != null)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: InkWell(
              onTap: task.employeeId == null ? null : () => context.push('/employee/${task.employeeId}'),
              child: Text('Led by ${_leader(context)}', style: TextStyle(color: Tokens.text2)),
            ),
          ),
        if (task.delegatedBy != null)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(
              'Delegated by ${task.delegatedBy}${_parentTitle() == null ? '' : ' for “${_parentTitle()}”'}',
              style: TextStyle(color: Tokens.accentText, fontSize: 13.5),
            ),
          ),
        for (final h in task.handoffs)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text('Handed over: ${h.from} → ${h.to}${h.reason.isEmpty ? '' : ' (${h.reason})'}', style: TextStyle(color: Tokens.text2, fontSize: 13.5)),
          ),
        if (task.scheduleId != null)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Row(children: [
              Icon(Icons.event_repeat, size: 16, color: Tokens.text2),
              const SizedBox(width: 6),
              Text('From a recurring task', style: TextStyle(color: Tokens.text2, fontSize: 13.5)),
            ]),
          ),
        if (task.run.choices.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Row(
              children: [
                Icon(Icons.tune, size: 16, color: Tokens.text2),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(
                    [
                      if (task.run.mode != null) {for (final (v, l) in taskModes) v: l}[task.run.mode!.wire] ?? task.run.mode!.wire,
                      if (task.run.model != null) ModelCatalog.friendly(task.run.model!),
                      if (task.run.effort != null) ModelCatalog.effortLabel(task.run.effort),
                      // Chosen for this task only; its conversation's model chip goes back to the employee's default.
                      if (task.run.model != null || task.run.effort != null) 'override',
                    ].join(' · '),
                    style: TextStyle(color: Tokens.text2, fontSize: 13.5),
                  ),
                ),
              ],
            ),
          ),
        if (task.conversationId != null)
          Align(
            alignment: Alignment.centerLeft,
            child: Padding(
              padding: const EdgeInsets.only(top: 8),
              child: OutlinedButton.icon(
                onPressed: task.employeeId == null ? null : () => context.push('/conversation/${task.conversationId}'),
                icon: const Icon(Icons.forum_outlined, size: 18),
                label: const Text('Open conversation'),
              ),
            ),
          ),
        if (task.isTicketMain)
          Align(
            alignment: Alignment.centerLeft,
            child: Padding(
              padding: const EdgeInsets.only(top: 8),
              child: OutlinedButton.icon(
                key: const Key('task-open-ticket'),
                onPressed: () => context.push('/ticket/${task.ticketId}'),
                icon: const Icon(Icons.view_kanban_outlined, size: 18),
                label: Text('Main task of ticket${task.ticketNumber == null ? '' : ' #${task.ticketNumber}'} · owner and cancel on the board'),
              ),
            ),
          ),
        if (task.canAssign)
          Align(
            alignment: Alignment.centerLeft,
            child: Padding(
              padding: const EdgeInsets.only(top: 8),
              child: OutlinedButton.icon(
                onPressed: busy ? null : () => _assign(context),
                icon: const Icon(Icons.person_outline, size: 18),
                label: Text(task.employeeId == null ? 'Give to an employee' : 'Give to someone else'),
              ),
            ),
          ),
        if (task.pendingApprovals > 0) ...[
          const SizedBox(height: 16),
          AttentionCard(
            onTap: () => context.go('/inbox'),
            child: Row(
              children: [
                Icon(Icons.pan_tool_outlined, color: Tokens.accentText, size: 20),
                const SizedBox(width: 12),
                Expanded(
                  child: Text(
                    task.pendingApprovals == 1 ? 'An approval or an answer waits for you.' : '${task.pendingApprovals} approvals or answers wait for you.',
                    style: TextStyle(color: Tokens.accentText),
                  ),
                ),
                Icon(Icons.chevron_right, color: Tokens.accentText),
              ],
            ),
          ),
        ],
        const Eyebrow('Goal', padding: EdgeInsets.fromLTRB(0, 20, 0, 6)),
        Text(task.goal, style: const TextStyle(fontSize: 15, height: 1.5)),
        if (task.attachments.isNotEmpty) ...[
          const Eyebrow('Attachments', padding: EdgeInsets.fromLTRB(0, 20, 0, 6)),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final f in task.attachments) _Attachment(file: f),
            ],
          ),
        ],
        if (task.criteria.isNotEmpty) ...[
          const Eyebrow('Acceptance criteria', padding: EdgeInsets.fromLTRB(0, 20, 0, 6)),
          for (final c in task.criteria)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.radio_button_unchecked, size: 18, color: Tokens.muted),
                  const SizedBox(width: 8),
                  Expanded(child: Text(c)),
                ],
              ),
            ),
        ],
        if (task.children.isNotEmpty) ...[
          const Eyebrow('Delegated to colleagues', padding: EdgeInsets.fromLTRB(0, 20, 0, 6)),
          for (final sub in task.children.map((id) => tasks.where((t) => t.id == id).firstOrNull).nonNulls)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: SurfaceCard(
                padding: EdgeInsets.zero,
                child: ListTile(
                  title: Text(sub.title),
                  subtitle: Row(children: [
                    TaskStatusMark(sub),
                    Flexible(child: Text(' · ${sub.employeeName ?? 'Nobody'}', maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: Tokens.muted, fontSize: 12.5))),
                  ]),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => context.push('/project/${sub.projectId}/task/${sub.id}'),
                ),
              ),
            ),
        ],
        if (task.result?.isNotEmpty == true) ...[
          const Eyebrow('Report', padding: EdgeInsets.fromLTRB(0, 20, 0, 6)),
          MessageMarkdown(data: task.result!, scope: FileScopeTask(taskId: task.id), selectable: true),
          CopyButton(key: const Key('copy-report'), text: task.result!),
        ],
        const SizedBox(height: 24),
        Wrap(spacing: 8, runSpacing: 8, children: [
          if (!task.isDone && !task.isTicketMain)
            OutlinedButton(
              style: OutlinedButton.styleFrom(foregroundColor: Tokens.negative),
              onPressed: busy
                  ? null
                  : () async {
                      if (await confirm(context, title: 'Cancel this task?', body: 'Running work stops.', action: 'Cancel task', danger: true)) bloc.cancelTask(task.id);
                    },
              child: const Text('Cancel task'),
            ),
        ]),
      ],
    ),
    );
  }
}
