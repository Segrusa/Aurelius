import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/widgets/dialogs.dart';
import '../../core/widgets/refresh.dart';
import '../../core/widgets/widgets.dart';
import '../../data/commands.dart';
import '../../data/company.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import 'idea_list.dart';
import 'idea_page.dart';
import 'idea_sheet.dart';
import 'ideas_bloc.dart';
import 'start_mode_sheet.dart';

/// "New idea" (the + in the project's app bar) for the nearest [IdeasBloc]. The company-wide
/// list has no project of its own: pass the [projectId] the idea goes to.
Future<void> addIdea(BuildContext context, {String? projectId}) => _editIdea(context, projectId: projectId);

Future<void> _editIdea(BuildContext context, {IdeaView? idea, String? projectId}) async {
  final bloc = context.read<IdeasBloc>();
  final project = projectId ?? bloc.projectId;
  // An idea always belongs to a project.
  assert(idea != null || project != null, 'a new idea needs a project');
  final edit = await IdeaPage.open(context, idea: idea, attachments: bloc.attachmentsEnabled);
  if (edit == null) return;
  bloc.add(IdeasCommandSent(
    idea == null
        ? Commands.addIdea(project!, edit.title, edit.note, kind: edit.kind, files: edit.files)
        : Commands.updateIdea(idea.id, edit.title, edit.note, kind: edit.kind, addFiles: edit.files, removeFiles: edit.removeFiles),
    done: idea == null ? 'Idea noted.' : 'Saved.',
  ));
}

/// Ideas: note one, start it as a task for an employee, or drop it. The [IdeasBloc] comes from
/// the project page (that project's ideas) or from the Ideas tab (every project's, each labelled
/// with its project); either way an action works on the idea's own project.
class IdeasTab extends StatelessWidget {
  const IdeasTab({super.key});

  Future<void> _actions(BuildContext context, IdeaView idea) async {
    final bloc = context.read<IdeasBloc>();
    final company = context.read<CompanyBloc>().state;
    // The management team first, then everyone else.
    final employees = [for (final g in pickerGroups(company.activeEmployees.where((m) => m.status == LifecycleStatus.active).toList())) ...g.employees];
    final choice = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      builder: (sheet) => BlocProvider.value(value: bloc, child: IdeaSheet(idea: idea, employees: employees, canDelete: bloc.attachmentsEnabled)),
    );
    if (choice == null || !context.mounted) return;
    // The sheet showed the latest version; act on that one.
    final latest = bloc.state.ideas.where((i) => i.id == idea.id).firstOrNull ?? idea;
    if (choice.startsWith('start:')) {
      final parts = choice.split(':');
      final name = parts.sublist(2).join(':');
      final variants = await bloc.variantsSetting();
      if (!context.mounted) return;
      final picked = await pickStartMode(context, employeeName: name, canAuto: bloc.canStartInAuto, maxVariants: variants?.enabled == true ? variants!.maxVariants : 1);
      if (picked == null) return;
      bloc.add(IdeasCommandSent(
        Commands.startIdea(idea.id, parts[1], mode: picked.mode, variants: picked.variants),
        done: picked.variants > 1 ? 'Started as a task for $name, tried ${picked.variants} ways first.' : 'Started as a task for $name.',
      ));
    } else if (choice == 'task') {
      context.push('/project/${idea.projectId}/task/${idea.taskId}');
    } else if (choice == 'edit') {
      await _editIdea(context, idea: latest);
    } else if (choice == 'delete') {
      final files = latest.attachments.length;
      final ok = await confirm(
        context,
        title: 'Delete this idea?',
        body: '"${latest.title}"${files > 0 ? ' and its ${files == 1 ? 'file' : '$files files'}' : ''} will be removed for good.${latest.taskId != null ? ' Its task stays.' : ''}',
        action: 'Delete',
        danger: true,
      );
      if (ok) bloc.add(IdeasCommandSent(Commands.deleteIdea(idea.id), done: 'Deleted.'));
    } else {
      bloc.add(IdeasCommandSent(Commands.setIdeaStatus(idea.id, IdeaStatus.fromJson(choice)), done: choice == 'dropped' ? 'Dropped.' : null));
    }
  }

  @override
  Widget build(BuildContext context) {
    return BlocConsumer<IdeasBloc, IdeasState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, state) {
        toast(context, state.notice!);
        context.read<IdeasBloc>().add(const IdeasNoticeShown());
      },
      builder: (context, state) {
        final bloc = context.read<IdeasBloc>();
        final ideas = state.visible;
        final company = context.read<CompanyBloc>().state;
        return Column(
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
              child: SizedBox(
                width: double.infinity,
                child: SegmentedButton<IdeaStatus>(
                  showSelectedIcon: false,
                  segments: [
                    ButtonSegment(value: IdeaStatus.open, label: Text(state.open > 0 ? 'Open (${state.open})' : 'Open')),
                    const ButtonSegment(value: IdeaStatus.done, label: Text('Done')),
                    const ButtonSegment(value: IdeaStatus.dropped, label: Text('Dropped')),
                  ],
                  selected: {state.filter},
                  onSelectionChanged: (s) => bloc.add(IdeasFilterChanged(s.first)),
                ),
              ),
            ),
            Padding(
                padding: const EdgeInsets.fromLTRB(16, 4, 16, 0),
                child: SizedBox(
                  width: double.infinity,
                  child: SegmentedButton<IdeaKind?>(
                    showSelectedIcon: false,
                    segments: const [
                      ButtonSegment(value: null, label: Text('All')),
                      ButtonSegment(value: IdeaKind.feature, label: Text('Features')),
                      ButtonSegment(value: IdeaKind.bug, label: Text('Bugs')),
                    ],
                    selected: {state.kind},
                    onSelectionChanged: (s) => bloc.add(IdeasKindChanged(s.first)),
                  ),
                ),
              ),
            Expanded(
              child: state.loading && state.ideas.isEmpty
                  ? const Center(child: CircularProgressIndicator())
                  : PullToRefresh(
                      scrollable: ideas.isNotEmpty,
                      onRefresh: () => untilReloaded(bloc, () => bloc.add(const IdeasRefreshed()), busy: (s) => s.loading),
                      child: ideas.isEmpty
                          ? EmptyState(switch (state.filter) {
                              IdeaStatus.open => 'No open ideas',
                              IdeaStatus.done => 'Nothing done yet',
                              IdeaStatus.dropped => 'Nothing dropped',
                            }, detail: state.filter == IdeaStatus.open ? 'Note one with +. Employees note ideas here too while they work.' : null)
                          : IdeaList(
                              ideas: ideas,
                              projectName: bloc.projectId == null ? (idea) => company.project(idea.projectId)?.name ?? 'Unknown project' : null,
                              onTap: (idea) => _actions(context, idea),
                              onDrop: (idea) => bloc.add(IdeasCommandSent(Commands.setIdeaStatus(idea.id, IdeaStatus.dropped), done: 'Dropped.')),
                            ),
                    ),
            ),
          ],
        );
      },
    );
  }
}
