import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../guidelines/target_guidelines_page.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../core/widgets/refresh.dart';
import '../../data/aurelius_repository.dart';
import '../../data/commands.dart';
import '../../data/views.dart';
import '../connection/connection_banner.dart';
import '../connection/connection_bloc.dart';
import '../ideas/ideas_bloc.dart';
import '../ideas/ideas_tab.dart';
import '../board/dispatcher_row.dart';
import '../company/company_bloc.dart';
import '../company_org/employee_widgets.dart';
import 'project_bloc.dart';
import 'project_edit.dart';
import 'task_status_mark.dart';
import 'tasks_tab.dart';

/// A project with tabs Overview, Tasks and Ideas. Employees are company-wide;
/// the overview shows who works here (employees with open tasks in it).
class ProjectPage extends StatelessWidget {
  const ProjectPage({super.key, required this.projectId, this.initialTab});

  final String projectId;
  final String? initialTab;

  static const tabs = ['overview', 'tasks', 'ideas'];

  @override
  Widget build(BuildContext context) {
    // Ideas live here, not in their tab, so the + in the app bar can add to
    // them and the overview can show them; they load when first read.
    return MultiBlocProvider(
      providers: [
        BlocProvider(create: (context) => ProjectBloc(context.read<AureliusRepository>(), projectId)),
        BlocProvider(create: (context) => IdeasBloc(context.read<AureliusRepository>(), projectId)),
      ],
      child: DefaultTabController(
        length: tabs.length,
        initialIndex: tabs.indexOf(initialTab ?? '').clamp(0, tabs.length - 1),
        child: BlocConsumer<ProjectBloc, ProjectState>(
          listenWhen: (a, b) => (b.notice != null && a.notice != b.notice) || (b.lastResult != null && a.lastResult != b.lastResult),
          listener: (context, state) {
            if (state.notice != null) toast(context, state.notice!);
            final result = state.lastResult;
            if (result is CommandResultTaskDiscussed) context.push('/conversation/${result.conversationId}?e=${result.employeeId}');
            context.read<ProjectBloc>().add(const ProjectNoticeShown());
          },
          builder: (context, state) {
            final project = state.project;
            return Scaffold(
              appBar: GlassAppBar(
                title: Text(project?.name ?? '', maxLines: 1, overflow: TextOverflow.ellipsis),
                actions: [if (project != null) _ProjectActions(project: project)],
                bottom: const PreferredSize(preferredSize: Size.fromHeight(52), child: _ProjectTabs()),
              ),
              body: Column(
                children: [
                  const ConnectionBanner(),
                  Expanded(
                    child: state.loading && project == null
                        ? const Center(child: CircularProgressIndicator())
                        : TabBarView(
                            // On iOS a swipe from the left edge goes back; tabs switch by tapping.
                            physics: Theme.of(context).platform == TargetPlatform.iOS ? const NeverScrollableScrollPhysics() : null,
                            children: [
                              _Overview(state: state),
                              const TasksTab(),
                              const IdeasTab(),
                            ],
                          ),
                  ),
                ],
              ),
            );
          },
        ),
      ),
    );
  }
}

/// The project's tabs as a pill segmented control under the title.
class _ProjectTabs extends StatelessWidget {
  const _ProjectTabs();

  static const labels = ['Overview', 'Tasks', 'Ideas'];

  @override
  Widget build(BuildContext context) {
    final tabs = DefaultTabController.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
      child: AnimatedBuilder(
        animation: tabs,
        builder: (context, _) => PillSegments<int>(
          scrollable: true,
          segments: [for (final (i, l) in labels.indexed) (i, l)],
          selected: tabs.index,
          onChanged: tabs.animateTo,
        ),
      ),
    );
  }
}

/// What the + in the app bar adds on each tab (by index into
/// [ProjectPage.tabs]); Overview has none.
typedef ProjectAdd = ({String tooltip, Future<void> Function(BuildContext) add});

ProjectAdd? projectAdd(int tab) => switch (ProjectPage.tabs[tab]) {
      'tasks' => (tooltip: 'New task', add: createTask),
      'ideas' => (tooltip: 'New idea', add: addIdea),
      _ => null,
    };

/// Top right: the project menu, and on a tab that adds something a + beside
/// it, in one glass capsule.
class _ProjectActions extends StatelessWidget {
  const _ProjectActions({required this.project});

  final ProjectSummary project;

  @override
  Widget build(BuildContext context) {
    final tabs = DefaultTabController.of(context);
    return AnimatedBuilder(
      animation: tabs,
      builder: (context, _) {
        final add = projectAdd(tabs.index);
        return GlassCapsule(
          children: [
            _ProjectMenu(project: project),
            if (add != null)
              IconButton(
                key: ValueKey('project-add-${ProjectPage.tabs[tabs.index]}'),
                tooltip: add.tooltip,
                icon: Icon(CupertinoIcons.add, color: Tokens.accent),
                onPressed: () => add.add(context),
              ),
          ],
        );
      },
    );
  }
}

class _ProjectMenu extends StatelessWidget {
  const _ProjectMenu({required this.project});

  final ProjectSummary project;

  @override
  Widget build(BuildContext context) {
    final bloc = context.read<ProjectBloc>();
    return GlassMenuButton<String>(
      tooltip: 'Project actions',
      onSelected: (value) async {
        switch (value) {
          case 'edit':
            await editProject(context, project);
          case 'pause':
            bloc.send(Commands.setProjectStatus(project.id, LifecycleStatus.paused), done: 'Project paused. Running work finishes; nothing new starts.');
          case 'resume':
            bloc.send(Commands.setProjectStatus(project.id, LifecycleStatus.active), done: 'Project resumed.');
          case 'archive':
            if (await confirm(context, title: 'Archive ${project.name}?', body: 'Its tasks stop. History stays readable.', action: 'Archive', danger: true)) {
              bloc.send(Commands.setProjectStatus(project.id, LifecycleStatus.archived), done: 'Project archived.', retry: false);
              if (context.mounted) context.pop();
            }
        }
      },
      items: () => [
        const GlassMenuItem('edit', 'Edit…'),
        if (project.status == ProjectStatus.active) const GlassMenuItem('pause', 'Pause'),
        if (project.status == ProjectStatus.paused) const GlassMenuItem('resume', 'Resume'),
        const GlassMenuItem('archive', 'Archive', danger: true),
      ],
    );
  }
}

class _Overview extends StatelessWidget {
  const _Overview({required this.state});

  final ProjectState state;

  @override
  Widget build(BuildContext context) {
    final project = state.project;
    if (project == null) return const EmptyState('This project no longer exists');
    final waiting = state.filtered(TaskFilter.waiting).where((t) => t.needsYou).toList();
    final running = state.filtered(TaskFilter.active);
    final company = context.watch<CompanyBloc>().state;
    final working = company.employeesIn(project.id);
    final open = <String, int>{};
    for (final t in state.tasks) {
      if (t.isOpen && t.employeeId != null) open.update(t.employeeId!, (n) => n + 1, ifAbsent: () => 1);
    }
    void tab(String name) => DefaultTabController.of(context).animateTo(ProjectPage.tabs.indexOf(name));
    return CustomScrollView(
      physics: const AlwaysScrollableScrollPhysics(parent: BouncingScrollPhysics()),
      slivers: [
        CupertinoSliverRefreshControl(
          onRefresh: () => untilReloaded(context.read<ProjectBloc>(), () => context.read<ProjectBloc>().add(const ProjectRefreshed()), busy: (s) => s.loading),
        ),
        SliverList(
          delegate: SliverChildListDelegate([
            if (project.description?.isNotEmpty == true)
              Padding(padding: const EdgeInsets.fromLTRB(32, 8, 32, 0), child: Text(project.description!, style: TextStyle(fontSize: 15, color: Tokens.label2))),
            if (project.folder != null)
              Padding(
                padding: const EdgeInsets.fromLTRB(32, 6, 32, 0),
                child: Text(project.folder!, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 12, color: Tokens.label3)),
              ),
            if (project.status == ProjectStatus.paused)
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 16, 16, 0),
                child: SurfaceCard(
                  padding: const EdgeInsets.fromLTRB(16, 8, 8, 8),
                  child: Row(
                    children: [
                      Icon(CupertinoIcons.pause_circle, color: Tokens.warning),
                      const SizedBox(width: 10),
                      const Expanded(child: Text('This project is paused. Nothing new starts.')),
                      TextButton(
                        onPressed: () => context.read<ProjectBloc>().send(Commands.setProjectStatus(project.id, LifecycleStatus.active), done: 'Project resumed.'),
                        child: const Text('Resume'),
                      ),
                    ],
                  ),
                ),
              ),
            const GroupHeader('Board'),
            ProjectDispatcherRow(project: project),
            if (context.select<ConnectionBloc, bool>((b) => b.state.has('vault'))) ...[
              const SizedBox(height: 12),
              InsetGroup(children: [
                GroupRow(
                  key: const ValueKey('project-vault'),
                  leading: const IconTile(CupertinoIcons.lock_fill),
                  title: 'Vault',
                  chevron: true,
                  onTap: () => context.push('/vault?project=${project.id}'),
                ),
              ]),
            ],
            if (context.select<ConnectionBloc, bool>((b) => b.state.has('guidelines'))) ...[
              const SizedBox(height: 12),
              InsetGroup(children: [GuidelinesRow(key: ValueKey('project-guidelines-${project.id}'), target: GuidelineTargetProject(id: project.id))]),
            ],
            if (waiting.isNotEmpty) ...[
              const GroupHeader('Needs Your Attention'),
              InsetGroup(
                children: [
                  for (final t in waiting)
                    GroupRow(
                      onTap: () => context.push('/project/${project.id}/task/${t.id}'),
                      title: t.title,
                      subtitle: t.statusLabel,
                      leading: Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: Tokens.accent)),
                      trailing: TaskStatusMark(t),
                    ),
                ],
              ),
            ],
            GroupHeader(working.isEmpty ? 'Who Works Here' : 'Who Works Here · ${working.length}'),
            InsetGroup(
              key: const ValueKey('who-works-here'),
              dividerIndent: 68,
              children: [
                if (working.isEmpty) const _Muted('Nobody has open work here'),
                for (final m in working)
                  GroupRow(
                    key: ValueKey('works-here-${m.id}'),
                    onTap: () => context.push('/employee/${m.id}'),
                    leading: Monogram(m.monogram, size: 40, active: m.working),
                    title: m.name,
                    subtitle: [m.roleLine, '${open[m.id] ?? 0} open task${open[m.id] == 1 ? '' : 's'}'].where((s) => s.isNotEmpty).join(' · '),
                    trailing: employeePresence(m),
                  ),
              ],
            ),
            GroupHeader(running.isEmpty ? 'Tasks' : 'Tasks · ${running.length} open', action: 'See All', onAction: () => tab('tasks')),
            InsetGroup(
              children: [
                if (running.isEmpty) const _Muted('No open tasks'),
                for (final t in running.take(6))
                  GroupRow(
                    onTap: () => context.push('/project/${project.id}/task/${t.id}'),
                    title: t.title,
                    subtitle: taskMarkDetail(t) ?? t.statusLabel,
                    trailing: TaskStatusMark(t),
                  ),
                InkWell(
                  onTap: () => createTask(context),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 13),
                    child: Row(children: [
                      Icon(CupertinoIcons.add, size: 20, color: Tokens.accent),
                      const SizedBox(width: 10),
                      Text('New Task', style: TextStyle(fontSize: 17, color: Tokens.accent)),
                    ]),
                  ),
                ),
              ],
            ),
            BlocBuilder<IdeasBloc, IdeasState>(
              builder: (context, ideas) {
                final open = ideas.ideas.where((i) => i.status == IdeaStatus.open).toList();
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    GroupHeader(open.isEmpty ? 'Ideas' : 'Ideas · ${open.length} open', action: 'See All', onAction: () => tab('ideas')),
                    InsetGroup(
                      children: [
                        if (open.isEmpty) _Muted(ideas.loading ? 'Loading…' : 'No open ideas'),
                        for (final i in open.take(5))
                          GroupRow(onTap: () => tab('ideas'), chevron: false, title: i.title, titleLines: 2, subtitle: '${i.author} · ${Fmt.clock(i.createdAtMs)}'),
                      ],
                    ),
                  ],
                );
              },
            ),
            SizedBox(height: listBottom(context)),
          ]),
        ),
      ],
    );
  }
}

class _Muted extends StatelessWidget {
  const _Muted(this.text);

  final String text;

  @override
  Widget build(BuildContext context) =>
      Padding(padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 13), child: Text(text, style: TextStyle(fontSize: 17, color: Tokens.label2)));
}
