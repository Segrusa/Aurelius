import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/refresh.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import 'task_status_mark.dart';
import 'project_bloc.dart';

class TasksTab extends StatefulWidget {
  const TasksTab({super.key});

  @override
  State<TasksTab> createState() => _TasksTabState();
}

class _TasksTabState extends State<TasksTab> {
  TaskFilter _filter = TaskFilter.active;

  @override
  Widget build(BuildContext context) {
    final state = context.watch<ProjectBloc>().state;
    final tasks = state.filtered(_filter);
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
          child: SizedBox(
            width: double.infinity,
            child: SegmentedButton<TaskFilter>(
              showSelectedIcon: false,
              segments: [
                for (final f in TaskFilter.values)
                  ButtonSegment(value: f, label: Text('${f.name[0].toUpperCase()}${f.name.substring(1)} ${state.filtered(f).length}')),
              ],
              selected: {_filter},
              onSelectionChanged: (s) => setState(() => _filter = s.first),
            ),
          ),
        ),
        Expanded(
          child: PullToRefresh(
            scrollable: tasks.isNotEmpty,
            onRefresh: () => untilReloaded(context.read<ProjectBloc>(), () => context.read<ProjectBloc>().add(const ProjectRefreshed()), busy: (s) => s.loading),
            child: tasks.isEmpty
                ? EmptyState(switch (_filter) {
                    TaskFilter.active => 'Nothing running',
                    TaskFilter.waiting => 'Nothing waits for you',
                    TaskFilter.completed => 'Nothing finished yet',
                  })
                : ListView.separated(
                    padding: EdgeInsets.only(bottom: listBottom(context)),
                    itemCount: tasks.length,
                    separatorBuilder: (_, _) => const Divider(height: 1, indent: 16),
                    itemBuilder: (context, i) => TaskRow(task: tasks[i]),
                  ),
          ),
        ),
      ],
    );
  }
}

class TaskRow extends StatelessWidget {
  const TaskRow({super.key, required this.task, this.company});

  final TaskView task;

  /// In company-wide lists: adds who leads it and the project.
  final CompanyState? company;

  @override
  Widget build(BuildContext context) {
    final detail = [
      if (company != null) ...[task.employeeName ?? 'Nobody', company!.project(task.projectId)?.name ?? ''],
      taskMarkDetail(task) ?? '',
    ].where((s) => s.isNotEmpty).join(' · ');
    return ListTile(
      // A task under a ticket says which (§2); one without a ticket stays as it was (B7).
      title: Text(task.ticketNumber == null ? task.title : '#${task.ticketNumber} · ${task.title}'),
      subtitle: Row(
        children: [
          TaskStatusMark(task),
          if (detail.isNotEmpty) Flexible(child: Text(' · $detail', maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: Tokens.muted, fontSize: 12.5))),
        ],
      ),
      trailing: Text(Fmt.ago(task.updatedAtMs), style: Tokens.monoSmall()),
      onTap: () => context.push('/project/${task.projectId}/task/${task.id}'),
    );
  }
}
