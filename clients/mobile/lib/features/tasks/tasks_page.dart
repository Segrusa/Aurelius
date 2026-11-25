import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/refresh.dart';
import '../../core/widgets/widgets.dart';
import '../../data/company.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import 'give_work.dart';
import '../project/tasks_tab.dart';

/// Which tasks the list shows by state.
enum TaskState { active, waiting, done }

/// Whether [t] is in [state]: needs you or waits to start; at work or queued; ended.
bool inTaskState(TaskView t, TaskState state) => switch (state) {
      TaskState.active => t.isActive,
      TaskState.waiting => t.isOpen && (t.needsYou || t.waitsToStart),
      TaskState.done => t.isDone && !t.hasLiveRun,
    };

/// Every task of every project, filtered by state, employee and project;
/// the newest first.
class TasksPage extends StatefulWidget {
  const TasksPage({super.key, this.employeeId});

  /// Start filtered on this employee (from its page).
  final String? employeeId;

  @override
  State<TasksPage> createState() => _TasksPageState();
}

class _TasksPageState extends State<TasksPage> {
  TaskState _state = TaskState.active;
  late String? _employee = widget.employeeId;
  String? _project;

  List<TaskView> _filtered(CompanyState company, {TaskState? state}) {
    return company.tasks.where((t) {
      if (!inTaskState(t, state ?? _state)) return false;
      if (_employee != null && t.employeeId != _employee) return false;
      if (_project != null && t.projectId != _project) return false;
      return true;
    }).toList()
      ..sort((a, b) => b.updatedAtMs.compareTo(a.updatedAtMs));
  }

  Future<void> _pick(String title, Map<String?, String> options, String? current, void Function(String?) set) async {
    final picked = await showModalBottomSheet<(String?,)>(
      context: context,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
          child: ListView(
            shrinkWrap: true,
            children: [
              Padding(padding: const EdgeInsets.fromLTRB(16, 16, 16, 4), child: Eyebrow(title)),
              for (final MapEntry(:key, :value) in options.entries)
                ListTile(
                  title: Text(value),
                  trailing: key == current ? Icon(Icons.check, color: Tokens.accentText) : null,
                  onTap: () => Navigator.pop(sheet, (key,)),
                ),
            ],
          ),
        ),
      ),
    );
    if (picked != null) setState(() => set(picked.$1));
  }

  @override
  Widget build(BuildContext context) {
    final company = context.watch<CompanyBloc>().state;
    final tasks = _filtered(company);
    final employees = [...company.activeEmployees]..sort(byName);
    final employee = company.employee(_employee ?? '');
    final project = company.project(_project ?? '');
    final bloc = context.read<CompanyBloc>();
    return Scaffold(
      appBar: GlassAppBar(
        title: const Text('Tasks'),
        actions: [GlassButton(tooltip: 'Give work', icon: CupertinoIcons.add, onPressed: () => giveWork(context, employeeId: _employee, projectId: _project))],
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
            child: PillSegments<TaskState>(
              segments: [
                for (final s in TaskState.values)
                  (s, '${switch (s) { TaskState.active => 'Active', TaskState.waiting => 'Waiting', TaskState.done => 'Done' }} ${_filtered(company, state: s).length}'),
              ],
              selected: _state,
              onChanged: (s) => setState(() => _state = s),
            ),
          ),
          SizedBox(
            height: 48,
            child: ListView(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
              children: [
                _FilterChip(
                  key: const ValueKey('tasks-filter-employee'),
                  label: employee?.name ?? 'Everyone',
                  active: employee != null,
                  onTap: () => _pick('Employee', {null: 'Everyone', for (final e in employees) e.id: e.name}, _employee, (v) => _employee = v),
                ),
                _FilterChip(
                  key: const ValueKey('tasks-filter-project'),
                  label: project?.name ?? 'All projects',
                  active: project != null,
                  onTap: () => _pick('Project', {null: 'All projects', for (final p in company.visibleProjects) p.id: p.name}, _project, (v) => _project = v),
                ),
              ],
            ),
          ),
          Expanded(
            child: PullToRefresh(
              scrollable: tasks.isNotEmpty,
              onRefresh: () => untilReloaded(bloc, () => bloc.add(const CompanyRefreshed()), busy: (s) => s.loading),
              child: tasks.isEmpty
                  ? EmptyState(switch (_state) {
                      TaskState.active => 'Nothing running',
                      TaskState.waiting => 'Nothing waits',
                      TaskState.done => 'Nothing finished yet',
                    })
                  : ListView.separated(
                      key: const ValueKey('company-tasks'),
                      padding: EdgeInsets.only(bottom: listBottom(context)),
                      itemCount: tasks.length,
                      separatorBuilder: (_, _) => const Divider(height: 1, indent: 16),
                      itemBuilder: (context, i) => TaskRow(task: tasks[i], company: company),
                    ),
            ),
          ),
        ],
      ),
    );
  }
}

class _FilterChip extends StatelessWidget {
  const _FilterChip({super.key, required this.label, required this.active, required this.onTap});

  final String label;
  final bool active;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(right: 8),
        child: FilterChip(
          label: Row(mainAxisSize: MainAxisSize.min, children: [Text(label), const SizedBox(width: 2), const Icon(Icons.expand_more, size: 16)]),
          selected: active,
          showCheckmark: false,
          onSelected: (_) => onTap(),
        ),
      );
}
