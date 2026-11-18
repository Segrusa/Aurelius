import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../core/widgets/refresh.dart';
import '../../data/views.dart';
import 'company_bloc.dart';

/// Projects: every project as a row with its status and who works
/// there; hold a row to drag it into another order (kept on this phone).
class ProjectsPage extends StatelessWidget {
  const ProjectsPage({super.key});

  Future<void> _newProject(BuildContext context) async {
    final values = await promptForm(
      context,
      title: 'New project',
      action: 'Create',
      fields: const [FieldSpec('Name', hint: 'For example Atlas'), FieldSpec('Description', required: false, lines: 2)],
    );
    if (values == null || !context.mounted) return;
    context.read<CompanyBloc>().add(ProjectCreated(values[0], description: values[1].isEmpty ? null : values[1]));
  }

  @override
  Widget build(BuildContext context) {
    return BlocConsumer<CompanyBloc, CompanyState>(
      listenWhen: (a, b) => (b.notice != null && a.notice != b.notice) || (b.createdProjectId != null && a.createdProjectId != b.createdProjectId),
      listener: (context, state) {
        if (state.notice != null) {
          toast(context, state.notice!);
        }
        if (state.createdProjectId != null) context.push('/project/${state.createdProjectId}');
        context.read<CompanyBloc>().add(const CompanyNoticeShown());
      },
      builder: (context, state) {
        final projects = state.visibleProjects;
        final waiting = projects.fold(0, (n, p) => n + state.waitingIn(p.id));
        final summary = [
          '${projects.length} project${projects.length == 1 ? '' : 's'}',
          if (waiting > 0) '$waiting waiting',
        ].join(' · ');
        return LargeTitleScaffold(
          title: 'Projects',
          subtitle: summary,
          actions: [GlassButton(tooltip: 'New project', icon: CupertinoIcons.add, onPressed: () => _newProject(context))],
          onRefresh: () => untilReloaded(context.read<CompanyBloc>(), () => context.read<CompanyBloc>().add(const CompanyRefreshed()), busy: (s) => s.loading),
          fill: state.loading && state.projects.isEmpty
              ? const Center(child: CupertinoActivityIndicator())
              : projects.isEmpty
                  ? EmptyState('No projects yet', action: FilledButton(onPressed: () => _newProject(context), child: const Text('New project')))
                  : null,
          slivers: [
            const SliverToBoxAdapter(child: SizedBox(height: 16)),
            if (projects.isNotEmpty)
              SliverPadding(
                padding: const EdgeInsets.symmetric(horizontal: 16),
                // Hold a row and drag it to change the order (kept on this phone).
                sliver: SliverReorderableList(
                  itemCount: projects.length,
                  onReorderStart: (_) => HapticFeedback.mediumImpact(),
                  onReorderItem: (from, to) => context.read<CompanyBloc>().add(ProjectsReordered(from, to)),
                  proxyDecorator: (child, _, animation) => AnimatedBuilder(
                    animation: animation,
                    builder: (context, child) => Material(
                      color: Colors.transparent,
                      elevation: 8 * Curves.easeOut.transform(animation.value),
                      shadowColor: const Color(0x33000000),
                      borderRadius: BorderRadius.circular(Tokens.cardRadius),
                      child: ClipRRect(borderRadius: BorderRadius.circular(Tokens.cardRadius), child: child),
                    ),
                    child: child,
                  ),
                  itemBuilder: (context, i) {
                    final p = projects[i];
                    return ReorderableDelayedDragStartListener(
                      key: ValueKey('project-${p.id}'),
                      index: i,
                      child: _ProjectCell(
                        first: i == 0,
                        last: i == projects.length - 1,
                        child: _ProjectRow(
                          project: p,
                          employees: state.employeesIn(p.id),
                          waiting: state.waitingIn(p.id),
                          active: state.tasks.where((t) => t.projectId == p.id && t.isOpen && !t.waitsToStart).length,
                        ),
                      ),
                    );
                  },
                ),
              ),
          ],
        );
      },
    );
  }
}

/// One row of the projects group, drawn as its own piece of an [InsetGroup]
/// (rounded ends, hairline between rows) so it can be dragged on its own.
class _ProjectCell extends StatelessWidget {
  const _ProjectCell({required this.first, required this.last, required this.child});

  final bool first;
  final bool last;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final r = Radius.circular(Tokens.cardRadius);
    return ClipRRect(
      borderRadius: BorderRadius.vertical(top: first ? r : Radius.zero, bottom: last ? r : Radius.zero),
      child: Material(
        color: Tokens.cell,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (!first) Divider(height: 0.5, thickness: 0.5, indent: 70, color: Tokens.sep),
            child,
          ],
        ),
      ),
    );
  }
}

class _ProjectRow extends StatelessWidget {
  const _ProjectRow({required this.project, required this.employees, required this.waiting, required this.active});

  final ProjectSummary project;

  /// Who works here: employees with open tasks in the project.
  final List<EmployeeSummary> employees;
  final int waiting;

  /// Tasks that have not ended yet.
  final int active;

  @override
  Widget build(BuildContext context) {
    final working = employees.where((m) => m.working).length;
    // Working employees first, then the rest, busiest first.
    final ordered = [...employees.where((m) => m.working), ...employees.where((m) => !m.working)];
    final names = ordered.take(2).map((m) => m.name).join(', ');
    final subtitle = [
      if (employees.isEmpty) 'Nobody at work here' else '${employees.length} employee${employees.length == 1 ? '' : 's'} active · $names',
      if (working > 0) '$working working',
      if (active > 0) '$active active task${active == 1 ? '' : 's'}',
    ].join(' · ');
    // One status per row, in the same order as desktop: Paused, Needs you, Working, Quiet.
    final Widget status = project.status == ProjectStatus.paused
        ? Text('Paused', style: TextStyle(color: Tokens.warning, fontSize: 15))
        : waiting > 0
            ? NeedsYouTag('Needs you · $waiting')
            : working > 0
                ? Row(mainAxisSize: MainAxisSize.min, children: [
                    const StatusDot(pulse: true, size: 7),
                    const SizedBox(width: 6),
                    Text('Working', style: TextStyle(color: Tokens.text2, fontSize: 15)),
                  ])
                : Text('Quiet', style: TextStyle(color: Tokens.muted, fontSize: 15));
    return GroupRow(
      onTap: () => context.push('/project/${project.id}'),
      padding: const EdgeInsets.fromLTRB(16, 11, 16, 11),
      leading: LetterTile(project.name.isEmpty ? '?' : project.name.characters.first.toUpperCase()),
      title: project.name,
      subtitle: subtitle,
      chevron: true,
      trailing: status,
    );
  }
}
